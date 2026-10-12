//! Platform-independent contracts for the retained desktop UI tree.

mod geometry;
pub use geometry::{PixelRect, ScaleFactor, Transform};
mod input;
pub use input::{
    DragEvent, DragFormat, DragItem, DragOperation, DragOperations, DropRouter, GestureEvent,
    HoverChange, HoverTracker, InputCapabilities, PointerDetails, PointerKind, negotiate_drag,
};
mod path;
pub use path::{
    FillRule, GradientStop, Image, ImageError, ImageSampling, LineCap, LineJoin, Paint, Path,
    PathElement, StrokeStyle, lerp_color,
};
mod blur;
pub use blur::blur_pixels;
mod display;
pub use display::{
    Color, CursorShape, DisplayCommand, DisplayList, DisplayListError, Shadow, TextRun, damage,
};
mod icons;
mod oklch;
mod svg;
pub use icons::Icon;
pub use svg::{SvgDocument, SvgError, SvgShape, parse_path_data};
mod layout;
pub use layout::{
    Axis, CrossAlign, FormLayout, GridLayout, Insets, LayoutDirection, LayoutError, LinearLayout,
};
mod layout_engine;
pub use layout_engine::{
    Constraints, EngineError, GridPlacement, ItemAlign, LayoutEngine, LayoutKind, LayoutStats,
    LayoutStyle, LeafMeasure, Length, MainAlign, Measured, Track,
};
mod scroll;
pub use scroll::ScrollViewport;
mod stack_window;
pub use stack_window::UniformStack;
mod semantics;
pub use semantics::{
    AnnouncementPriority, SemanticAction, SemanticChange, SemanticCollection, SemanticEntry,
    SemanticError, SemanticNode, SemanticRelation, SemanticRole, SemanticState, SemanticText,
    SemanticTree,
};
mod capabilities;
pub use capabilities::{BackendCapabilities, Support};
mod text;
pub use text::{TextBuffer, TextError, TextMotion, UNDO_LIMIT, grapheme_boundaries};
mod text_layout;
pub use text_layout::{
    CaretStop, FontSlant, FontWeight, GlyphRun, LayoutLine, TextCacheStats, TextDirection,
    TextLayout, TextLayoutCache, TextLayoutError, TextShaper, TextStyle, first_strong_is_rtl,
};
mod undo;
pub use undo::{UndoGroup, UndoStack};
mod theme;
pub use theme::{
    AccessibilityPreferences, BaseColor, ColorRole, ColorScheme, Contrast, DesignTokens,
    FULL_RADIUS, FontFamily, MotionTokens, Oklch, Palette, RadiusScale, RecipeSet, RoleOverrides,
    ShadowLayers, ShadowScale, Space, TextPreset, TextSize, ThemeError, ThemeInvalidation,
    ThemeOverride, ThemeResolver, Tracking, TypeSpec, palette_source,
};
mod locale;
pub use locale::{
    BasicCollator, Catalog, CatalogIssue, Collator, DateOrder, Locale, LocaleData, LocaleError,
    Localizer, Message, PluralCategory, fold_for_search,
};
mod datetime;
pub use datetime::{
    CivilDate, CivilDateTime, CivilTime, Weekday, days_in_month, is_leap_year, month_grid,
    month_name, weekday_short_name,
};
mod resource;
pub use resource::{ResourceBundle, ResourceError, ResourceKind, ResourceRef};
mod model;
pub use model::{
    ChangeLog, CheckState, EditableModel, ItemData, ItemKey, ItemModel, ListStore, ModelChange,
    ModelError, Role, Selection, SelectionCommand, SelectionMode, StringListModel, TreeStore,
    Value, visible_keys,
};
mod fs_model;
pub use fs_model::{
    DirectoryState, FS_COLUMN_KIND, FS_COLUMN_MODIFIED, FS_COLUMN_NAME, FS_COLUMN_SIZE, FileEntry,
    FileSystemModel, format_size,
};
mod proxy;
pub use proxy::{
    ConcatModel, MappedModel, MatchMode, ProxyView, SliceModel, SortFilterProxy, SortOrder,
    TextFilter,
};
mod rich_text;
pub use rich_text::{
    BlockAlignment, BlockKind, FormatFlag, RichBlock, RichDocument, RichPosition, RichRun,
    RichTextError, SpanFormat,
};
mod rich_layout;
pub use rich_layout::{RichLayout, RichLine, RichPiece, RichTheme};
mod scene;
pub use scene::{ItemShape, ItemStyle, Scene, SceneChange, SceneError, SceneItem, SceneItemId};
mod settings;
pub use settings::{
    LoadReport, SettingValue, Settings, SettingsError, SettingsMigration, SettingsSchema,
    WindowGeometry, is_valid_setting_key, user_settings_path,
};
mod notify;
pub use notify::{
    Notification, NotificationQueue, NotifyError, TimerId, WorkError, WorkQueue, WorkSender,
};
mod animation;
pub use animation::{
    AnimationClock, AnimationError, AnimationFrame, AnimationId, AnimationKey, AnimationSpec,
    AnimationStart, AnimationUpdate, AnimationValues, CubicBezier, Easing,
};

use std::collections::HashMap;

/// A position in logical points. Platform backends apply display scale only at
/// the boundary where they paint or receive device coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An extent in logical points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// A half-open rectangle in logical points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub fn contains(self, point: Point) -> bool {
        self.size.width > 0.0
            && self.size.height > 0.0
            && point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x - self.origin.x < self.size.width
            && point.y - self.origin.y < self.size.height
    }
}

/// A stable identity for a node during its lifetime. IDs are never reused by a tree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeError {
    MissingParent,
    CannotDetachRoot,
    MissingNode,
    InvalidGeometry,
    HiddenNode,
    InvalidChildIndex,
    Cycle,
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingParent => f.write_str("the parent node is not in the tree"),
            Self::CannotDetachRoot => f.write_str("the root node cannot be detached"),
            Self::MissingNode => f.write_str("the node is not in the tree"),
            Self::InvalidGeometry => f.write_str("bounds are not finite or have a negative size"),
            Self::HiddenNode => f.write_str("a hidden node cannot take focus or pointer capture"),
            Self::InvalidChildIndex => f.write_str("the child index is out of range"),
            Self::Cycle => f.write_str("the move would make a node its own ancestor"),
        }
    }
}

impl std::error::Error for TreeError {}

/// A logical pointer button. Backends map OS-specific button numbers here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Other(usize),
}

/// Units for wheel displacement. Controls resolve line units using their
/// current text metrics; precise devices report logical points directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WheelUnit {
    Points,
    Lines,
}

/// Owned pointer data in logical points. Backends convert device coordinates
/// before creating an event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvent {
    Down {
        position: Point,
        button: PointerButton,
    },
    Up {
        position: Point,
        button: PointerButton,
    },
    Move {
        position: Point,
    },
    Wheel {
        position: Point,
        /// Positive values request scrolling right or down.
        delta: Point,
        unit: WheelUnit,
    },
}

impl PointerEvent {
    pub const fn position(self) -> Point {
        match self {
            Self::Down { position, .. }
            | Self::Up { position, .. }
            | Self::Move { position }
            | Self::Wheel { position, .. } => position,
        }
    }
}

/// Logical keys used for control navigation. Text entry is a separate IME path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Tab,
    Enter,
    Space,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    /// A printable key, lowercased and independent of modifiers, from the
    /// active keyboard layout (so shortcuts follow AZERTY or Dvorak).
    Character(char),
    /// Platform key code for keys without a portable name.
    Other(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyEvent {
    Down { key: Key, repeat: bool },
    Up { key: Key },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventReply {
    Continue,
    Stop,
}

/// Work pending for a node. A layout change also requires repainting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dirty(u8);

impl Dirty {
    pub const NONE: Self = Self(0);
    pub const LAYOUT: Self = Self(1);
    pub const PAINT: Self = Self(2);
    pub const SEMANTICS: Self = Self(4);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The work of both values.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

struct Node {
    bounds: Rect,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    dirty: Dirty,
    focusable: bool,
    visible: bool,
    /// Whether hit testing can find the node and its subtree.
    hit_testable: bool,
}

/// Retained ownership and geometry. Bounds are absolute logical coordinates;
/// children are clipped to every ancestor's bounds during hit testing.
pub struct Tree {
    root: NodeId,
    next_id: u64,
    nodes: HashMap<NodeId, Node>,
    focus: Option<NodeId>,
    modality: InputModality,
    pointer_capture: Option<NodeId>,
}

/// The kind of input that last moved or used focus. Focus that keyboard
/// input caused shows a focus ring; focus from a pointer press does not.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum InputModality {
    #[default]
    Keyboard,
    Pointer,
}

impl Tree {
    pub fn new(bounds: Rect) -> Self {
        let root = NodeId(0);
        let nodes = HashMap::from([(
            root,
            Node {
                bounds,
                parent: None,
                children: Vec::new(),
                dirty: Dirty(Dirty::LAYOUT.0 | Dirty::PAINT.0 | Dirty::SEMANTICS.0),
                focusable: false,
                visible: true,
                hit_testable: true,
            },
        )]);
        Self {
            root,
            next_id: 1,
            nodes,
            focus: None,
            modality: InputModality::Keyboard,
            pointer_capture: None,
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.get(&id).and_then(|node| node.parent)
    }

    /// True when `id` is `ancestor` or lies in its subtree.
    pub fn is_within(&self, id: NodeId, ancestor: NodeId) -> bool {
        let mut current = Some(id);
        while let Some(node) = current {
            if node == ancestor {
                return true;
            }
            current = self.parent(node);
        }
        false
    }

    pub fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.nodes.get(&id).map(|node| node.children.as_slice())
    }

    /// Places an attached node at `index` under `parent`, preserving its
    /// identity and subtree. All validation completes before mutation.
    pub fn place_child(
        &mut self,
        parent: NodeId,
        child: NodeId,
        index: usize,
    ) -> Result<(), TreeError> {
        if child == self.root {
            return Err(TreeError::CannotDetachRoot);
        }
        if !self.contains(child) {
            return Err(TreeError::MissingNode);
        }
        let parent_node = self.nodes.get(&parent).ok_or(TreeError::MissingParent)?;
        let old_parent = self.nodes[&child]
            .parent
            .expect("non-root node must have a parent");
        let mut ancestor = Some(parent);
        while let Some(id) = ancestor {
            if id == child {
                return Err(TreeError::Cycle);
            }
            ancestor = self.nodes[&id].parent;
        }
        let current_index = parent_node.children.iter().position(|id| *id == child);
        let maximum = parent_node.children.len() - usize::from(current_index.is_some());
        if index > maximum {
            return Err(TreeError::InvalidChildIndex);
        }
        if old_parent == parent && current_index == Some(index) {
            return Ok(());
        }
        self.nodes
            .get_mut(&old_parent)
            .expect("parent must exist")
            .children
            .retain(|id| *id != child);
        self.nodes
            .get_mut(&parent)
            .expect("validated parent must exist")
            .children
            .insert(index, child);
        self.nodes.get_mut(&child).expect("child must exist").parent = Some(parent);
        let dirty = Dirty(Dirty::LAYOUT.0 | Dirty::PAINT.0 | Dirty::SEMANTICS.0);
        self.mark_ancestors(old_parent, dirty);
        self.mark_ancestors(parent, dirty);
        Ok(())
    }

    pub fn attach(&mut self, parent: NodeId, bounds: Rect) -> Result<NodeId, TreeError> {
        let parent_node = self
            .nodes
            .get_mut(&parent)
            .ok_or(TreeError::MissingParent)?;
        let id = NodeId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("node identity exhausted");
        parent_node.children.push(id);
        self.nodes.insert(
            id,
            Node {
                bounds,
                parent: Some(parent),
                children: Vec::new(),
                dirty: Dirty(Dirty::LAYOUT.0 | Dirty::PAINT.0 | Dirty::SEMANTICS.0),
                focusable: false,
                visible: true,
                hit_testable: true,
            },
        );
        self.mark_ancestors(parent, Dirty::LAYOUT);
        self.mark_ancestors(parent, Dirty::PAINT);
        self.mark_ancestors(parent, Dirty::SEMANTICS);
        Ok(id)
    }

    pub fn detach(&mut self, id: NodeId) -> Result<(), TreeError> {
        if id == self.root {
            return Err(TreeError::CannotDetachRoot);
        }
        let parent = self
            .nodes
            .get(&id)
            .ok_or(TreeError::MissingNode)?
            .parent
            .unwrap();
        self.nodes
            .get_mut(&parent)
            .unwrap()
            .children
            .retain(|child| *child != id);
        let mut pending = vec![id];
        while let Some(current) = pending.pop() {
            if let Some(node) = self.nodes.remove(&current) {
                if self.focus == Some(current) {
                    self.focus = None;
                }
                if self.pointer_capture == Some(current) {
                    self.pointer_capture = None;
                }
                pending.extend(node.children);
            }
        }
        self.mark_ancestors(parent, Dirty::LAYOUT);
        self.mark_ancestors(parent, Dirty::PAINT);
        self.mark_ancestors(parent, Dirty::SEMANTICS);
        Ok(())
    }

    pub fn bounds(&self, id: NodeId) -> Option<Rect> {
        self.nodes.get(&id).map(|node| node.bounds)
    }

    pub fn set_bounds(&mut self, id: NodeId, bounds: Rect) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode)?;
        if node.bounds != bounds {
            node.bounds = bounds;
            self.mark_ancestors(id, Dirty::LAYOUT);
            self.mark_ancestors(id, Dirty::PAINT);
            self.mark_ancestors(id, Dirty::SEMANTICS);
        }
        Ok(())
    }

    /// Places a node as the result of layout. Unlike [`Tree::set_bounds`],
    /// this does not request layout again; it requests paint and semantics.
    pub fn set_layout_bounds(&mut self, id: NodeId, bounds: Rect) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode)?;
        if node.bounds != bounds {
            node.bounds = bounds;
            self.mark_ancestors(id, Dirty(Dirty::PAINT.0 | Dirty::SEMANTICS.0));
        }
        Ok(())
    }

    /// Moves a node and every descendant in the same logical coordinate
    /// space. Geometry is validated before any node changes, so a failed move
    /// leaves bounds, focus, and dirty flags untouched.
    pub fn translate_subtree(&mut self, id: NodeId, delta: Point) -> Result<(), TreeError> {
        if !self.contains(id) {
            return Err(TreeError::MissingNode);
        }
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return Err(TreeError::InvalidGeometry);
        }
        if delta == Point::new(0.0, 0.0) {
            return Ok(());
        }
        let mut pending = vec![id];
        let mut translated = Vec::new();
        while let Some(current) = pending.pop() {
            let node = self
                .nodes
                .get(&current)
                .expect("attached descendant must exist");
            let origin = Point::new(
                node.bounds.origin.x + delta.x,
                node.bounds.origin.y + delta.y,
            );
            if !origin.x.is_finite()
                || !origin.y.is_finite()
                || !(origin.x + node.bounds.size.width).is_finite()
                || !(origin.y + node.bounds.size.height).is_finite()
            {
                return Err(TreeError::InvalidGeometry);
            }
            translated.push((current, origin));
            pending.extend(node.children.iter().copied());
        }
        let geometry = Dirty(Dirty::PAINT.0 | Dirty::SEMANTICS.0);
        for (current, origin) in translated {
            let node = self
                .nodes
                .get_mut(&current)
                .expect("attached descendant must exist");
            node.bounds.origin = origin;
            node.dirty.insert(geometry);
        }
        self.mark_ancestors(id, geometry);
        Ok(())
    }

    pub fn take_dirty(&mut self, id: NodeId) -> Result<Dirty, TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode)?;
        Ok(std::mem::replace(&mut node.dirty, Dirty::NONE))
    }

    /// Requests work for this node and its ancestors without changing geometry.
    pub fn invalidate(&mut self, id: NodeId, dirty: Dirty) -> Result<(), TreeError> {
        if !self.contains(id) {
            return Err(TreeError::MissingNode);
        }
        self.mark_ancestors(id, dirty);
        Ok(())
    }

    pub fn focus(&self) -> Option<NodeId> {
        self.focus
    }

    /// True when `id` has focus and the last input was the keyboard: the
    /// control shows its focus ring.
    pub fn focus_visible(&self, id: NodeId) -> bool {
        self.focus == Some(id) && self.modality == InputModality::Keyboard
    }

    pub fn input_modality(&self) -> InputModality {
        self.modality
    }

    /// Records the kind of the latest input. A change repaints the focused
    /// node, because its ring appears or disappears.
    pub fn set_input_modality(&mut self, modality: InputModality) {
        if self.modality != modality {
            self.modality = modality;
            if let Some(focus) = self.focus {
                self.mark_ancestors(focus, Dirty::PAINT);
            }
        }
    }

    /// Focuses `id` because of a pointer press: no focus ring shows.
    pub fn focus_by_pointer(&mut self, id: NodeId) -> Result<(), TreeError> {
        self.set_focus(Some(id))?;
        self.set_input_modality(InputModality::Pointer);
        Ok(())
    }

    pub fn visible(&self, id: NodeId) -> Option<bool> {
        self.nodes.get(&id)?;
        let mut current = Some(id);
        while let Some(current_id) = current {
            let node = &self.nodes[&current_id];
            if !node.visible {
                return Some(false);
            }
            current = node.parent;
        }
        Some(true)
    }

    pub fn set_visible(&mut self, id: NodeId, visible: bool) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode)?;
        if node.visible == visible {
            return Ok(());
        }
        node.visible = visible;
        if !visible {
            if self
                .focus
                .is_some_and(|focused| self.is_descendant_or_self(focused, id))
            {
                self.focus = None;
            }
            if self
                .pointer_capture
                .is_some_and(|captured| self.is_descendant_or_self(captured, id))
            {
                self.pointer_capture = None;
            }
        }
        self.mark_ancestors(id, Dirty(Dirty::PAINT.0 | Dirty::SEMANTICS.0));
        Ok(())
    }

    /// Sets whether pointer hit testing can find the node and its
    /// subtree. The nodes still paint, for example while a closed panel
    /// fades out, and presses go to the nodes below them. Pointer
    /// capture inside the subtree ends.
    pub fn set_hit_testable(&mut self, id: NodeId, hit_testable: bool) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode)?;
        node.hit_testable = hit_testable;
        if !hit_testable
            && self
                .pointer_capture
                .is_some_and(|captured| self.is_descendant_or_self(captured, id))
        {
            self.pointer_capture = None;
        }
        Ok(())
    }

    /// Marks a node for keyboard traversal. Traversal follows depth-first
    /// child order and wraps; containers remain skipped unless opted in.
    pub fn set_focusable(&mut self, id: NodeId, focusable: bool) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode)?;
        if node.focusable != focusable {
            node.focusable = focusable;
            self.mark_ancestors(id, Dirty::SEMANTICS);
            if !focusable && self.focus == Some(id) {
                self.set_focus(None)?;
            }
        }
        Ok(())
    }

    /// Moves focus to the next (or previous) opted-in node, wrapping at ends.
    pub fn focus_next(&mut self, reverse: bool) -> Option<NodeId> {
        self.focus_next_within(self.root, reverse)
    }

    /// Moves focus to the next (or previous) opted-in node inside the
    /// subtree of `scope`, wrapping at its ends. A modal overlay uses its
    /// own node as the scope, so traversal stays inside it. Focus outside
    /// the scope moves to its first (or last) node.
    pub fn focus_next_within(&mut self, scope: NodeId, reverse: bool) -> Option<NodeId> {
        if !self.contains(scope) {
            return None;
        }
        self.set_input_modality(InputModality::Keyboard);
        let mut order = Vec::new();
        let mut pending = vec![scope];
        while let Some(id) = pending.pop() {
            let node = self.nodes.get(&id).expect("attached child must exist");
            if !node.visible {
                continue;
            }
            if node.focusable {
                order.push(id);
            }
            pending.extend(node.children.iter().rev().copied());
        }
        let next = if order.is_empty() {
            None
        } else if let Some(index) = self
            .focus
            .and_then(|id| order.iter().position(|candidate| *candidate == id))
        {
            Some(order[(index + if reverse { order.len() - 1 } else { 1 }) % order.len()])
        } else if reverse {
            order.last().copied()
        } else {
            order.first().copied()
        };
        self.set_focus(next).expect("focus target is attached");
        next
    }

    pub fn set_focus(&mut self, id: Option<NodeId>) -> Result<(), TreeError> {
        if id.is_some_and(|id| !self.contains(id)) {
            return Err(TreeError::MissingNode);
        }
        if id.is_some_and(|id| self.visible(id) != Some(true)) {
            return Err(TreeError::HiddenNode);
        }
        if self.focus != id {
            if let Some(old) = self.focus {
                self.mark_ancestors(old, Dirty::PAINT);
                self.mark_ancestors(old, Dirty::SEMANTICS);
            }
            self.focus = id;
            if let Some(new) = id {
                self.mark_ancestors(new, Dirty::PAINT);
                self.mark_ancestors(new, Dirty::SEMANTICS);
            }
        }
        Ok(())
    }

    pub fn pointer_capture(&self) -> Option<NodeId> {
        self.pointer_capture
    }

    pub fn set_pointer_capture(&mut self, id: Option<NodeId>) -> Result<(), TreeError> {
        if id.is_some_and(|id| !self.contains(id)) {
            return Err(TreeError::MissingNode);
        }
        if id.is_some_and(|id| self.visible(id) != Some(true)) {
            return Err(TreeError::HiddenNode);
        }
        self.pointer_capture = id;
        Ok(())
    }

    fn mark_ancestors(&mut self, start: NodeId, dirty: Dirty) {
        let mut current = Some(start);
        while let Some(id) = current {
            let node = self.nodes.get_mut(&id).expect("tree parent must exist");
            node.dirty.insert(dirty);
            current = node.parent;
        }
    }

    pub fn hit_test(&self, point: Point) -> Option<NodeId> {
        self.hit_node(self.root, point)
    }

    /// Dispatches one pointer event synchronously. Capture uses the path from
    /// root to the target's parent, then target, then bubbles back to root.
    /// A stopped event or removed target ends dispatch. Reentrant dispatch is
    /// depth-first: a nested event completes before the outer path resumes.
    /// An outer path skips nodes detached by a nested event.
    pub fn dispatch_pointer(
        &mut self,
        event: PointerEvent,
        mut handler: impl FnMut(&mut Self, NodeId, EventPhase, PointerEvent) -> EventReply,
    ) {
        if let PointerEvent::Down { .. } = event {
            self.set_input_modality(InputModality::Pointer);
        }
        let Some(target) = self
            .pointer_capture
            .or_else(|| self.hit_test(event.position()))
        else {
            return;
        };
        self.dispatch_path(target, event, &mut handler);
    }

    /// Dispatches one pointer event along the path to `target`, which the
    /// caller chose, for example a control host that routes overlays and
    /// modal scopes first. A missing target is ignored.
    pub fn dispatch_pointer_to(
        &mut self,
        target: NodeId,
        event: PointerEvent,
        mut handler: impl FnMut(&mut Self, NodeId, EventPhase, PointerEvent) -> EventReply,
    ) {
        if self.contains(target) {
            self.dispatch_path(target, event, &mut handler);
        }
    }

    /// Sends navigation keys to the focused node. Character insertion and IME
    /// composition use a separate text-input contract, not key events.
    pub fn dispatch_key(
        &mut self,
        event: KeyEvent,
        mut handler: impl FnMut(&mut Self, NodeId, EventPhase, KeyEvent) -> EventReply,
    ) {
        if let KeyEvent::Down { .. } = event {
            self.set_input_modality(InputModality::Keyboard);
        }
        if let Some(target) = self.focus {
            self.dispatch_path(target, event, &mut handler);
        }
    }

    fn dispatch_path<E: Copy>(
        &mut self,
        target: NodeId,
        event: E,
        handler: &mut impl FnMut(&mut Self, NodeId, EventPhase, E) -> EventReply,
    ) {
        let mut path = Vec::new();
        let mut current = Some(target);
        while let Some(id) = current {
            let node = self.nodes.get(&id).expect("dispatch target must exist");
            path.push(id);
            current = node.parent;
        }
        path.reverse();

        for id in path.iter().copied().take(path.len() - 1) {
            if !self.contains(target) || !self.contains(id) {
                return;
            }
            if handler(self, id, EventPhase::Capture, event) == EventReply::Stop {
                return;
            }
        }
        if !self.contains(target)
            || handler(self, target, EventPhase::Target, event) == EventReply::Stop
        {
            return;
        }
        for id in path.iter().rev().copied().skip(1) {
            if !self.contains(target) || !self.contains(id) {
                return;
            }
            if handler(self, id, EventPhase::Bubble, event) == EventReply::Stop {
                return;
            }
        }
    }

    fn hit_node(&self, id: NodeId, point: Point) -> Option<NodeId> {
        let node = self.nodes.get(&id)?;
        if !node.visible || !node.hit_testable || !node.bounds.contains(point) {
            return None;
        }
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hit_node(*child, point) {
                return Some(hit);
            }
        }
        Some(id)
    }

    fn is_descendant_or_self(&self, node: NodeId, ancestor: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.parent(id);
        }
        false
    }
}
