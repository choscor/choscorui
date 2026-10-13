//! The view runtime: it mounts a view tree on a retained [`Tree`], keeps
//! live properties up to date, and runs layout, paint, semantics, and
//! input for it.

use super::View;
use super::env::{Corner, Elevation, Env, Fill};
use super::fragment::{FragmentId, Fragments};
use super::layout::ScrollShared;
use super::lazy::LazyHook;
use super::present::Anchor;
use super::scope::{self, Owned};
use super::state::{Binding, Live, Memo, State};
use super::storage::StorageError;
use crate::reactive::{ReactiveError, Runtime, Subscription};
use choscorui_controls::{
    ControlHost, ControlKind, ControlMessage, CustomControl, HostEnv, ShortcutModifiers, Styles,
    TextMeasure,
};
use choscorui_core::{
    Axis, Constraints, DesignTokens, Dirty, DisplayList, EngineError, GridPlacement, InputModality,
    ItemAlign, Key, KeyEvent, LayoutDirection, LayoutEngine, LayoutKind, LayoutStyle, Length,
    Measured, NodeId, Point, PointerEvent, RecipeSet, Rect, SemanticAction, SemanticChange,
    SemanticEntry, SemanticError, SemanticNode, SemanticRole, SemanticTree, Size, ThemeError,
    ThemeOverride, ThemeResolver, Track, Tree, TreeError,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

/// What can go wrong while the runtime updates a view tree.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewError {
    Tree(TreeError),
    Layout(EngineError),
    Theme(ThemeError),
    Semantics(SemanticError),
    Reactive(ReactiveError),
    /// Two items of a `ForEach` have the same key; the later one is left
    /// out.
    DuplicateKey(String),
    /// A control view could not build its control, with the reason.
    Control(String),
    /// A `.toast` modifier has no `.toaster` above it.
    NoToaster,
    /// A view option or value was invalid, and the view used the nearest
    /// valid value instead, with the reason.
    Clamped(String),
    /// Stored state or the settings behind it had a problem.
    Storage(StorageError),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tree(error) => write!(f, "tree update failed: {error}"),
            Self::Layout(error) => write!(f, "layout failed: {error}"),
            Self::Theme(error) => write!(f, "theme update failed: {error}"),
            Self::Semantics(error) => write!(f, "semantics update failed: {error}"),
            Self::Reactive(error) => write!(f, "state update failed: {error}"),
            Self::DuplicateKey(key) => write!(f, "duplicate key {key} in a list"),
            Self::Control(reason) => write!(f, "control build failed: {reason}"),
            Self::NoToaster => write!(f, "a toast has no toaster above it"),
            Self::Clamped(reason) => write!(f, "invalid view option clamped: {reason}"),
            Self::Storage(error) => write!(f, "storage: {error}"),
        }
    }
}

impl std::error::Error for ViewError {}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ScopeId(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReactionId(u64);

/// Work that runs again when the state it read changes.
type ReactionFn = Rc<dyn Fn(&mut Inner)>;

struct Reaction {
    scope: ScopeId,
    run: ReactionFn,
    subscription: Option<Subscription>,
}

/// Runs when a scope unmounts, for example to remove a recipe set.
type Cleanup = Box<dyn FnOnce(&mut Inner)>;

struct ScopeData {
    parent: Option<ScopeId>,
    owned: Rc<RefCell<Owned>>,
    reactions: Vec<ReactionId>,
    children: Vec<ScopeId>,
    cleanups: Vec<Cleanup>,
}

/// One painted layer of a view, in modifier order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Layer {
    Background(Fill),
    Border(Fill, f64),
    Shadow(Elevation),
    /// An outline outside the bounds.
    Ring(Fill, f64),
}

/// How a node paints around its content.
#[derive(Clone, Debug)]
pub(crate) struct Decor {
    pub layers: Vec<Layer>,
    pub radius: Option<Corner>,
    pub opacity: f64,
    pub offset: Point,
    pub clip: bool,
}

impl Default for Decor {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            radius: None,
            opacity: 1.0,
            offset: Point::new(0.0, 0.0),
            clip: false,
        }
    }
}

/// Accessibility values that replace what the view reports.
#[derive(Clone, Debug, Default)]
pub(crate) struct Access {
    pub label: Option<String>,
    pub hint: Option<String>,
    pub value: Option<String>,
    pub hidden: bool,
    /// The text of a `tooltip` modifier, which describes the view when no
    /// hint does.
    pub tooltip: Option<String>,
    /// The description or error of the field around a control, which
    /// describes it when no hint or tooltip does.
    pub field: Option<String>,
    /// Whether the field around a control shows an error.
    pub invalid: bool,
    /// Relations from the field around a control to its label and text.
    pub relations: Vec<choscorui_core::SemanticRelation>,
}

impl Access {
    fn is_set(&self) -> bool {
        self.label.is_some()
            || self.hint.is_some()
            || self.value.is_some()
            || self.hidden
            || self.tooltip.is_some()
            || self.field.is_some()
            || self.invalid
            || !self.relations.is_empty()
    }
}

/// The rounds of message delivery after one input: a message, and the
/// messages that its handlers make controls emit.
const MESSAGE_ROUNDS: usize = 4;

/// A message handler for a control node.
pub(crate) type Handler = Rc<dyn Fn(&ControlMessage)>;

/// What the runtime keeps for each node it created.
#[derive(Default)]
pub(crate) struct NodeData {
    pub decor: Decor,
    pub access: Access,
    pub handlers: Vec<Handler>,
    pub control: bool,
    /// Theme overrides from environment modifiers.
    pub theme: ThemeOverride,
    pub themed: bool,
    /// The recipe set of local style modifiers on a control.
    pub instance: Option<RecipeSet>,
    /// Whether the node is a text leaf.
    pub text: bool,
    /// The kind whose root slot local style modifiers change.
    pub kind: Option<ControlKind>,
    /// How children sit in an overlay node: (vertical, horizontal).
    pub child_align: Option<(ItemAlign, ItemAlign)>,
    /// The column count of a grid node, which places children row by row.
    pub grid_columns: Option<usize>,
    /// The scroll state of a scroll view node.
    pub scroll: Option<Rc<ScrollShared>>,
    /// A handler that changes the runtime itself, such as scrolling.
    pub internal: Option<InternalHandler>,
    /// Width ÷ height that the node keeps.
    pub aspect: Option<f64>,
    /// The focus binding of a `focused` modifier.
    pub focus_binding: Option<Binding<bool>>,
    /// Whether the parent's child alignment set this node's alignment.
    pub placed: bool,
    /// The axes along which the node takes all the space it is offered.
    pub flex: Flex,
    /// Whether the node draws the focus ring of a descendant, as a
    /// `FocusFrame` view does.
    pub focus_frame: bool,
    /// Whether the node publishes no semantic node, as the wrapper of a
    /// tooltip or a context menu does.
    pub silent: bool,
    /// The tooltip text of a tooltip wrapper.
    pub tip: Option<Rc<RefCell<String>>>,
    /// The node of a tooltip wrapper that publishes its tooltip.
    pub bubble: Option<NodeId>,
    /// The tooltip wrapper and the view whose tooltip this node publishes
    /// while it shows.
    pub describes: Option<(NodeId, NodeId)>,
    /// Opens the context menu of the view, for the `ShowMenu` action.
    pub show_menu: Option<MenuOpener>,
    /// The hover card of a hover card wrapper.
    pub card: Option<Rc<super::hover_card::Card>>,
    /// The open and close motion of a presented panel, which moves the
    /// node and its subtree when it paints.
    pub motion: Option<Rc<super::motion::Motion>>,
}

/// The axes along which a view takes all the space it is offered, as a
/// scroll view or a frame with an infinite maximum does. A stack or an
/// overlay with a flexible child is flexible along the same axes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Flex {
    pub wide: bool,
    pub tall: bool,
}

impl Flex {
    pub const BOTH: Self = Self {
        wide: true,
        tall: true,
    };

    fn any(self) -> bool {
        self.wide || self.tall
    }
}

/// Opens the context menu of a view.
pub(crate) type MenuOpener = Rc<dyn Fn(&Inner)>;

/// The context menu that a secondary press opens after dispatch.
pub(crate) type MenuPress = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// A message handler that changes the runtime.
pub(crate) type InternalHandler = Rc<dyn Fn(&mut Inner, &ControlMessage)>;

/// A keyboard shortcut declared on a view.
pub(crate) struct ShortcutEntry {
    pub node: NodeId,
    pub key: Key,
    pub modifiers: ShortcutModifiers,
    pub action: Rc<dyn Fn()>,
}

/// The runtime's state. Live properties change it through reactions.
pub(crate) struct Inner {
    pub tree: Tree,
    pub engine: LayoutEngine,
    pub controls: ControlHost,
    pub themes: ThemeResolver,
    pub styles: Styles,
    pub runtime: Runtime,
    pub text: Rc<dyn TextMeasure>,
    pub nodes: HashMap<NodeId, NodeData>,
    pub fragments: Fragments,
    scopes: HashMap<ScopeId, ScopeData>,
    reactions: HashMap<ReactionId, Reaction>,
    queue: Rc<RefCell<Vec<ReactionId>>>,
    waker: Option<Rc<dyn Fn()>>,
    next_scope: u64,
    next_reaction: u64,
    next_recipe: u32,
    pub semantics: SemanticTree,
    semantic_dirty: BTreeSet<NodeId>,
    semantic_all: bool,
    touched: HashSet<NodeId>,
    pub needs_layout: bool,
    pub needs_paint: bool,
    pub direction: LayoutDirection,
    pub time: f64,
    pub shortcuts: Vec<ShortcutEntry>,
    errors: Vec<ViewError>,
    /// Nodes whose children changed, for child alignment and grid cells.
    arrange: BTreeSet<NodeId>,
    /// Scroll view nodes, whose shared state follows layout.
    pub scrolls: BTreeSet<NodeId>,
    /// Nodes that keep an aspect ratio.
    pub aspects: BTreeSet<NodeId>,
    /// The focus that bindings and scroll views last saw.
    focus: Option<NodeId>,
    /// A focused node to scroll into view after layout.
    reveal: Option<NodeId>,
    /// Anchored presentation layers and what they sit next to.
    pub anchors: HashMap<NodeId, Anchor>,
    /// The track nodes of lazy stacks, and the work that mounts their
    /// visible items after layout.
    pub lazies: BTreeMap<NodeId, LazyHook>,
    /// The hover and focus tooltips of every tooltip wrapper, shared so
    /// that a second tooltip follows the first without a delay.
    pub tooltips: Rc<RefCell<super::tooltip::Tooltips>>,
    /// The node that published the shown tooltip at the last semantics
    /// update.
    tooltip_node: Option<NodeId>,
    /// The context menu that the last secondary press opens: the one of
    /// the innermost view, which sees the press last.
    pub menu_press: MenuPress,
}

/// The first recipe set that instances use, above any set an
/// application registers by hand.
const INSTANCE_SETS: u32 = 1 << 24;

/// The most layout passes in one frame: aspect ratios, scrolling a
/// focused control into view, and the items that a lazy stack mounts
/// need another one.
const LAYOUT_PASSES: usize = 4;

/// The most rounds of reactions in one flush.
const MAX_ROUNDS: usize = 1_000;

impl Inner {
    pub(crate) fn report(&mut self, error: ViewError) {
        self.errors.push(error);
    }

    /// Marks `node` changed: it is counted, and the tree and layout
    /// engine learn what to redo.
    pub(crate) fn touch(&mut self, node: NodeId, dirty: Dirty) {
        if !self.tree.contains(node) {
            return;
        }
        self.touched.insert(node);
        let _ = self.tree.invalidate(node, dirty);
        if dirty.contains(Dirty::LAYOUT) {
            self.engine.invalidate(&self.tree, node);
            self.needs_layout = true;
        }
        if dirty.contains(Dirty::SEMANTICS) {
            self.semantic_dirty.insert(node);
        }
        self.needs_paint = true;
    }

    /// Records that the children of `parent` changed, so the next layout
    /// places them in its alignment or grid cells.
    pub(crate) fn children_changed(&mut self, parent: NodeId) {
        self.arrange.insert(parent);
        self.needs_layout = true;
    }

    /// Gives the children of each changed node the node's child alignment
    /// and grid cells.
    fn arrange_children(&mut self) {
        for parent in std::mem::take(&mut self.arrange) {
            let Some(data) = self.nodes.get(&parent) else {
                continue;
            };
            let (align, columns) = (data.child_align, data.grid_columns);
            if align.is_none() && columns.is_none() {
                continue;
            }
            let children = self.tree.children(parent).map(<[NodeId]>::to_vec);
            let children = children.unwrap_or_default();
            if let Some((vertical, horizontal)) = align {
                for child in &children {
                    let placed = self.nodes.get(child).is_some_and(|data| data.placed);
                    let mut style = self.engine.style(*child).cloned().unwrap_or_default();
                    if placed || (style.align_self.is_none() && style.justify_self.is_none()) {
                        style.align_self = Some(vertical);
                        style.justify_self = Some(horizontal);
                        self.set_style(*child, style);
                        self.data(*child).placed = true;
                        self.apply_flex(*child);
                    }
                }
            }
            if let Some(columns) = columns {
                let rows = children.len().div_ceil(columns.max(1));
                let mut style = self.engine.style(parent).cloned().unwrap_or_default();
                if let LayoutKind::Grid { rows: tracks, .. } = &mut style.kind {
                    *tracks = vec![Track::Auto; rows];
                }
                self.set_style(parent, style);
                for (index, child) in children.iter().enumerate() {
                    let mut style = self.engine.style(*child).cloned().unwrap_or_default();
                    style.grid = GridPlacement::cell(index / columns, index % columns);
                    self.set_style(*child, style);
                }
            }
        }
    }

    /// Sets the layout style of `node` when it changed.
    pub(crate) fn set_style(&mut self, node: NodeId, style: LayoutStyle) {
        if self.engine.style(node) == Some(&style) {
            return;
        }
        if let Err(error) = self.engine.set_style(&self.tree, node, style) {
            self.report(ViewError::Layout(error));
        }
        self.engine.invalidate(&self.tree, node);
        self.needs_layout = true;
    }

    /// Updates scroll views, aspect ratios, and lazy stacks after layout. Returns true
    /// when the layout has to run again.
    fn after_layout(&mut self) -> bool {
        let mut again = false;
        for node in self.aspects.clone() {
            let Some(ratio) = self.nodes.get(&node).and_then(|data| data.aspect) else {
                continue;
            };
            let width = self
                .tree
                .bounds(node)
                .map_or(0.0, |bounds| bounds.size.width);
            let height = Length::Points((width / ratio).round());
            let mut style = self.engine.style(node).cloned().unwrap_or_default();
            if style.height != height {
                style.height = height;
                self.set_style(node, style);
                again = true;
            }
        }
        for (layer, anchor) in self.anchors.clone() {
            again |= super::present::place_anchored(self, layer, &anchor);
        }
        if let Some(node) = self.reveal.take() {
            again |= self.reveal_node(node);
        }
        for node in &self.scrolls {
            if let Some(shared) = self.nodes.get(node).and_then(|data| data.scroll.as_ref()) {
                shared.offset.set(self.engine.scroll_offset(*node));
                let bounds = self.tree.bounds(*node).unwrap_or_default();
                shared
                    .content
                    .set(self.engine.content_size(*node).unwrap_or(bounds.size));
            }
        }
        let hooks: Vec<LazyHook> = self.lazies.values().cloned().collect();
        for hook in hooks {
            again |= hook(self);
        }
        again
    }

    /// Scrolls every scroll view around `node` so that `node` shows.
    /// Returns true when one moved.
    fn reveal_node(&mut self, node: NodeId) -> bool {
        let Some(target) = self.tree.bounds(node) else {
            return false;
        };
        let mut moved = false;
        let mut current = self.tree.parent(node);
        while let Some(id) = current {
            if self.scrolls.contains(&id)
                && let Some(view) = self.tree.bounds(id)
            {
                let offset = self.engine.scroll_offset(id);
                let mut next = offset;
                if target.min_y() < view.min_y() {
                    next.y -= view.min_y() - target.min_y();
                } else if target.max_y() > view.max_y() {
                    next.y += (target.max_y() - view.max_y()).min(target.min_y() - view.min_y());
                }
                if target.min_x() < view.min_x() {
                    next.x -= view.min_x() - target.min_x();
                } else if target.max_x() > view.max_x() {
                    next.x += (target.max_x() - view.max_x()).min(target.min_x() - view.min_x());
                }
                if next != offset {
                    super::layout::set_scroll(self, id, next);
                    moved = true;
                }
            }
            current = self.tree.parent(id);
        }
        moved
    }

    /// Updates focus bindings after focus moved, and asks layout to show
    /// the newly focused node.
    fn sync_focus(&mut self) {
        let focus = self.tree.focus();
        if focus == self.focus {
            return;
        }
        let old = std::mem::replace(&mut self.focus, focus);
        for (node, value) in [(old, false), (focus, true)] {
            if let Some(binding) = node
                .and_then(|node| self.nodes.get(&node))
                .and_then(|data| data.focus_binding)
                && binding.try_get().is_ok_and(|current| current != value)
            {
                binding.set(value);
            }
        }
        super::tooltip::follow_focus(self, old, focus);
        super::hover_card::follow_focus(self, old, focus);
        if focus.is_some() {
            self.reveal = focus;
            self.needs_layout = true;
        }
    }

    /// Moves layout, paint, and semantics requests that controls made
    /// into the runtime's flags. Only paths with requests are walked.
    fn sync_control_dirt(&mut self) {
        let root = self.tree.root();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            let dirty = self.tree.take_dirty(node).unwrap_or(Dirty::NONE);
            if dirty == Dirty::NONE {
                continue;
            }
            if dirty.contains(Dirty::LAYOUT) {
                self.needs_layout = true;
                if self.controls.contains(node) {
                    self.engine.invalidate(&self.tree, node);
                }
            }
            if dirty.contains(Dirty::SEMANTICS) && self.nodes.contains_key(&node) {
                self.semantic_dirty.insert(node);
            }
            self.needs_paint = true;
            if let Some(children) = self.tree.children(node) {
                stack.extend_from_slice(children);
            }
        }
    }

    pub(crate) fn data(&mut self, node: NodeId) -> &mut NodeData {
        self.nodes.entry(node).or_default()
    }

    /// Makes `node` take the space it is offered along the axes of `flex`,
    /// and its stack and overlay ancestors with it, up to an ancestor with
    /// a fixed size along that axis.
    pub(crate) fn make_flexible(&mut self, node: NodeId, flex: Flex) {
        let root = self.tree.root();
        let (mut node, mut flex) = (node, flex);
        while flex.any() && node != root {
            let have = self.data(node).flex;
            let added = Flex {
                wide: flex.wide && !have.wide,
                tall: flex.tall && !have.tall,
            };
            if !added.any() {
                return;
            }
            self.data(node).flex = Flex {
                wide: have.wide || added.wide,
                tall: have.tall || added.tall,
            };
            self.apply_flex(node);
            let Some(parent) = self.tree.parent(node) else {
                return;
            };
            let style = self.engine.style(parent).cloned().unwrap_or_default();
            if !matches!(style.kind, LayoutKind::Stack { .. } | LayoutKind::Overlay) {
                return;
            }
            flex = Flex {
                wide: added.wide && style.width == Length::Auto,
                tall: added.tall && style.height == Length::Auto,
            };
            node = parent;
        }
    }

    /// Gives `node` the alignment and growth of its flexibility in its
    /// parent.
    pub(crate) fn apply_flex(&mut self, node: NodeId) {
        let flex = self
            .nodes
            .get(&node)
            .map(|data| data.flex)
            .unwrap_or_default();
        if !flex.any() {
            return;
        }
        let Some(parent) = self.tree.parent(node) else {
            return;
        };
        let kind = self.engine.style(parent).map(|style| style.kind.clone());
        let mut style = self.engine.style(node).cloned().unwrap_or_default();
        match kind {
            Some(LayoutKind::Stack { axis, .. }) => {
                let (along, across) = match axis {
                    Axis::Horizontal => (flex.wide, flex.tall),
                    Axis::Vertical => (flex.tall, flex.wide),
                };
                if along {
                    style.grow = style.grow.max(1.0);
                }
                if across {
                    style.align_self = Some(ItemAlign::Stretch);
                }
            }
            // In an overlay, `align_self` is vertical and `justify_self`
            // horizontal.
            Some(LayoutKind::Overlay) => {
                let (vertical, horizontal) = self
                    .nodes
                    .get(&parent)
                    .and_then(|data| data.child_align)
                    .unwrap_or((ItemAlign::Center, ItemAlign::Center));
                let vertical = style.align_self.unwrap_or(vertical);
                let horizontal = style.justify_self.unwrap_or(horizontal);
                style.align_self = Some(if flex.tall {
                    ItemAlign::Stretch
                } else {
                    vertical
                });
                style.justify_self = Some(if flex.wide {
                    ItemAlign::Stretch
                } else {
                    horizontal
                });
            }
            _ => return,
        }
        self.set_style(node, style);
    }

    /// A recipe set that no other node uses.
    pub(crate) fn new_recipe_set(&mut self) -> RecipeSet {
        let set = RecipeSet(INSTANCE_SETS + self.next_recipe);
        self.next_recipe += 1;
        set
    }

    /// Applies the node's merged theme overrides to the resolver.
    pub(crate) fn sync_theme(&mut self, node: NodeId) {
        let Some(data) = self.nodes.get(&node) else {
            return;
        };
        let mut overrides = data.theme;
        if let Some(set) = data.instance {
            overrides.recipes = Some(set);
        }
        let result = if data.themed || data.instance.is_some() {
            self.themes.set_scope(&mut self.tree, node, overrides)
        } else {
            self.themes.remove_scope(&mut self.tree, node)
        };
        match result {
            Ok(change) => {
                if change == choscorui_core::ThemeInvalidation::Layout {
                    self.engine.invalidate_all();
                    self.needs_layout = true;
                }
                self.touched.insert(node);
                self.needs_paint = true;
            }
            Err(error) => self.report(ViewError::Theme(error)),
        }
    }

    fn new_scope(&mut self, parent: Option<ScopeId>) -> ScopeId {
        let id = ScopeId(self.next_scope);
        self.next_scope += 1;
        self.scopes.insert(
            id,
            ScopeData {
                parent,
                owned: Rc::default(),
                reactions: Vec::new(),
                children: Vec::new(),
                cleanups: Vec::new(),
            },
        );
        if let Some(parent) = parent.and_then(|parent| self.scopes.get_mut(&parent)) {
            parent.children.push(id);
        }
        id
    }

    pub(crate) fn child_scope(&mut self, parent: ScopeId) -> ScopeId {
        self.new_scope(Some(parent))
    }

    pub(crate) fn owned(&self, scope: ScopeId) -> Rc<RefCell<Owned>> {
        self.scopes
            .get(&scope)
            .map(|scope| scope.owned.clone())
            .unwrap_or_default()
    }

    pub(crate) fn on_dispose(&mut self, scope: ScopeId, cleanup: Cleanup) {
        if let Some(scope) = self.scopes.get_mut(&scope) {
            scope.cleanups.push(cleanup);
        }
    }

    /// Stops a scope, its nested scopes, and their reactions, and frees
    /// what they own.
    pub(crate) fn dispose_scope(&mut self, id: ScopeId) {
        let Some(mut data) = self.scopes.remove(&id) else {
            return;
        };
        for child in std::mem::take(&mut data.children) {
            self.dispose_scope(child);
        }
        for reaction in data.reactions {
            if let Some(reaction) = self.reactions.remove(&reaction)
                && let Some(subscription) = reaction.subscription
            {
                self.runtime.unsubscribe(subscription);
            }
        }
        for cleanup in data.cleanups {
            cleanup(self);
        }
        data.owned.borrow_mut().dispose();
        if let Some(parent) = data.parent.and_then(|parent| self.scopes.get_mut(&parent)) {
            parent.children.retain(|child| *child != id);
        }
    }

    /// Adds work that runs now and again whenever state it read changes.
    pub(crate) fn react(&mut self, scope: ScopeId, run: impl Fn(&mut Inner) + 'static) {
        let id = ReactionId(self.next_reaction);
        self.next_reaction += 1;
        self.reactions.insert(
            id,
            Reaction {
                scope,
                run: Rc::new(run),
                subscription: None,
            },
        );
        if let Some(scope) = self.scopes.get_mut(&scope) {
            scope.reactions.push(id);
        }
        self.run_reaction(id);
    }

    fn run_reaction(&mut self, id: ReactionId) {
        let Some(reaction) = self.reactions.get(&id) else {
            return;
        };
        let run = reaction.run.clone();
        let owned = self.owned(reaction.scope);
        let runtime = self.runtime.clone();
        let ((), reads) = runtime.track(|| {
            let _owner = scope::enter(owned);
            run(self)
        });
        let queue = self.queue.clone();
        let waker = self.waker.clone();
        if let Some(reaction) = self.reactions.get_mut(&id) {
            if let Some(old) = reaction.subscription.take() {
                runtime.unsubscribe(old);
            }
            if !reads.is_empty() {
                reaction.subscription = Some(runtime.observe(&reads, move || {
                    queue.borrow_mut().push(id);
                    if let Some(waker) = &waker {
                        waker();
                    }
                }));
            }
        }
    }

    /// Detaches `nodes` and everything below them, and forgets them.
    pub(crate) fn remove_nodes(&mut self, nodes: &[NodeId]) {
        let mut removed = Vec::new();
        for node in nodes {
            if !self.tree.contains(*node) {
                continue;
            }
            let mut stack = vec![*node];
            while let Some(current) = stack.pop() {
                removed.push(current);
                if let Some(children) = self.tree.children(current) {
                    stack.extend_from_slice(children);
                }
            }
            if let Some(parent) = self.tree.parent(*node) {
                self.touch(parent, Dirty::LAYOUT);
                self.children_changed(parent);
                self.semantic_all = true;
            }
            if let Err(error) = self.tree.detach(*node) {
                self.report(ViewError::Tree(error));
            }
        }
        for node in &removed {
            if let Some(set) = self.nodes.remove(node).and_then(|data| data.instance) {
                self.styles.remove_set(set);
            }
            self.touched.remove(node);
            self.semantic_dirty.remove(node);
            self.scrolls.remove(node);
            self.aspects.remove(node);
            self.arrange.remove(node);
            self.anchors.remove(node);
            self.lazies.remove(node);
        }
        let tree = &self.tree;
        self.anchors
            .retain(|layer, anchor| tree.contains(*layer) && anchor.exists(tree));
        if self.focus.is_some_and(|focus| !self.tree.contains(focus)) {
            self.focus = None;
        }
        self.shortcuts
            .retain(|entry| self.tree.contains(entry.node));
        self.controls.prune(&self.tree);
        self.engine.prune(&self.tree);
        self.themes.prune(&self.tree);
        self.fragments.prune(&self.tree);
        self.needs_layout = true;
        self.needs_paint = true;
    }
}

/// The mount context. A component's `body` receives it to create state,
/// memos, and effects, and to read the environment.
pub struct Cx<'a> {
    pub(crate) inner: &'a mut Inner,
    pub(crate) parent: NodeId,
    pub(crate) fragment: FragmentId,
    pub(crate) scope: ScopeId,
    pub(crate) env: Env,
}

impl Cx<'_> {
    /// State that the component owns, starting at `initial`.
    pub fn state<T: Clone + PartialEq + 'static>(&mut self, initial: T) -> State<T> {
        let property = self.inner.runtime.property(initial);
        let id = property.identity();
        let state = State::new(property);
        self.inner.owned(self.scope).borrow_mut().add_property(
            &self.inner.runtime,
            id,
            state.slot(),
        );
        state
    }

    /// A value computed from state. It computes again only when a value
    /// that it read last time changes.
    pub fn memo<T: Clone + PartialEq + 'static>(
        &mut self,
        compute: impl Fn() -> T + 'static,
    ) -> Memo<T> {
        let property = self.inner.runtime.computed(compute);
        let id = property.identity();
        let memo = Memo::new(property);
        self.inner.owned(self.scope).borrow_mut().add_property(
            &self.inner.runtime,
            id,
            memo.slot(),
        );
        memo
    }

    /// Runs `effect` now and after each change of the state it read, until
    /// the component unmounts.
    pub fn effect(&mut self, effect: impl Fn() + 'static) {
        let subscription = self.inner.runtime.effect(effect);
        self.inner
            .owned(self.scope)
            .borrow_mut()
            .add_effect(&self.inner.runtime, subscription);
    }

    /// The environment at this point of the tree.
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// The reactive runtime of the tree.
    pub fn runtime(&self) -> &Runtime {
        &self.inner.runtime
    }

    /// A context that mounts children of `node`, with `env`.
    pub(crate) fn within(&mut self, node: NodeId, env: Env) -> Cx<'_> {
        let fragment = self.inner.fragments.root_of(node);
        Cx {
            inner: &mut *self.inner,
            parent: node,
            fragment,
            scope: self.scope,
            env,
        }
    }

    /// Places a freshly attached `node` at the end of this context's
    /// fragment.
    fn place(&mut self, node: NodeId) {
        let index = self
            .inner
            .fragments
            .append_index(&self.inner.tree, self.fragment);
        if self
            .inner
            .tree
            .children(self.parent)
            .and_then(|c| c.get(index))
            != Some(&node)
            && let Err(error) = self.inner.tree.place_child(self.parent, node, index)
        {
            self.inner.report(ViewError::Tree(error));
        }
        self.inner.fragments.push_node(self.fragment, node);
        self.inner.nodes.entry(node).or_default();
        self.inner.children_changed(self.parent);
        self.inner.touch(node, Dirty::LAYOUT);
        self.inner.semantic_all = true;
    }

    /// Creates a plain node with `style`.
    pub(crate) fn node(&mut self, style: LayoutStyle) -> NodeId {
        let node = match self.inner.tree.attach(self.parent, Rect::default()) {
            Ok(node) => node,
            Err(error) => {
                self.inner.report(ViewError::Tree(error));
                return self.parent;
            }
        };
        self.place(node);
        if let Err(error) = self.inner.engine.set_style(&self.inner.tree, node, style) {
            self.inner.report(ViewError::Layout(error));
        }
        node
    }

    /// Mounts `control` on a new node.
    pub(crate) fn control(&mut self, control: Box<dyn CustomControl>) -> NodeId {
        let node = match self
            .inner
            .controls
            .mount(&mut self.inner.tree, self.parent, control)
        {
            Ok(node) => node,
            Err(error) => {
                self.inner.report(ViewError::Tree(error));
                return self.parent;
            }
        };
        self.place(node);
        self.inner.data(node).control = true;
        node
    }

    /// Applies `live` through `apply` now, and again whenever the state
    /// that it read changes.
    pub(crate) fn bind<T: Clone + 'static>(
        &mut self,
        live: Live<T>,
        apply: impl Fn(&mut Inner, T) + 'static,
    ) {
        match live {
            Live::Value(value) => apply(self.inner, value),
            Live::Computed(compute) => {
                self.inner
                    .react(self.scope, move |inner| apply(inner, compute()));
            }
        }
    }

    /// Runs `run` now and again whenever the state that it read changes.
    pub(crate) fn react(&mut self, run: impl Fn(&mut Inner) + 'static) {
        self.inner.react(self.scope, run);
    }

    /// Mounts `view` here without recording reads in any reaction.
    pub(crate) fn mount<V: View>(&mut self, view: V) {
        Runtime::untracked(|| view.mount(self));
    }
}

/// Mounts views on a retained tree and runs them: live properties,
/// layout, paint, semantics, and input. A window backend calls it each
/// frame; tests drive it headlessly.
pub struct ViewHost {
    pub(crate) inner: Inner,
    root_scope: Option<ScopeId>,
    env: Env,
    modifiers: ShortcutModifiers,
}

impl ViewHost {
    /// A runtime for a window of `size`, with application `tokens`, the
    /// root environment `env`, the reactive `runtime` that the window's
    /// state uses, and `text` measurement.
    pub fn new(
        size: Size,
        tokens: DesignTokens,
        env: Env,
        runtime: Runtime,
        text: Rc<dyn TextMeasure>,
    ) -> Self {
        let bounds = Rect::new(Point::new(0.0, 0.0), size);
        let tree = Tree::new(bounds);
        let inner = Inner {
            tree,
            engine: LayoutEngine::new(),
            controls: ControlHost::new(),
            themes: ThemeResolver::new(tokens),
            styles: Styles::new(),
            runtime,
            text,
            nodes: HashMap::new(),
            fragments: Fragments::default(),
            scopes: HashMap::new(),
            reactions: HashMap::new(),
            queue: Rc::default(),
            waker: None,
            next_scope: 0,
            next_reaction: 0,
            next_recipe: 0,
            semantics: SemanticTree::new(),
            semantic_dirty: BTreeSet::new(),
            semantic_all: true,
            touched: HashSet::new(),
            needs_layout: true,
            needs_paint: true,
            direction: LayoutDirection::Ltr,
            time: 0.0,
            shortcuts: Vec::new(),
            errors: Vec::new(),
            arrange: BTreeSet::new(),
            scrolls: BTreeSet::new(),
            aspects: BTreeSet::new(),
            focus: None,
            reveal: None,
            anchors: HashMap::new(),
            lazies: BTreeMap::new(),
            tooltips: Rc::default(),
            tooltip_node: None,
            menu_press: Rc::default(),
        };
        let mut host = Self {
            inner,
            root_scope: None,
            env,
            modifiers: ShortcutModifiers::default(),
        };
        let root = host.inner.tree.root();
        let style = LayoutStyle {
            kind: choscorui_core::LayoutKind::Overlay,
            ..LayoutStyle::default()
        };
        if let Err(error) = host.inner.engine.set_style(&host.inner.tree, root, style) {
            host.inner.report(ViewError::Layout(error));
        }
        // As in SwiftUI, the root view sits in the middle of the window
        // unless it asks for more space.
        host.inner.data(root).child_align = Some((ItemAlign::Center, ItemAlign::Center));
        host
    }

    /// Calls `waker` whenever state changes outside input handling, so
    /// the window can schedule a frame.
    pub fn set_waker(&mut self, waker: impl Fn() + 'static) {
        self.inner.waker = Some(Rc::new(waker));
    }

    /// Replaces the mounted view with `view`.
    pub fn mount(&mut self, view: impl View) {
        if let Some(scope) = self.root_scope.take() {
            self.inner.dispose_scope(scope);
            let root = self.inner.tree.root();
            let fragment = self.inner.fragments.root_of(root);
            let nodes = self.inner.fragments.clear(fragment);
            self.inner.remove_nodes(&nodes);
        }
        let scope = self.inner.new_scope(None);
        self.root_scope = Some(scope);
        let env = self.env.clone();
        let root = self.inner.tree.root();
        // The root follows the environment's layout direction.
        let direction_env = env.clone();
        self.inner.react(scope, move |inner| {
            let direction = direction_env.direction();
            if direction != inner.direction {
                inner.direction = direction;
                inner.engine.invalidate_all();
                inner.needs_layout = true;
                inner.needs_paint = true;
                inner.semantic_all = true;
            }
            let root = inner.tree.root();
            let mut style = inner.engine.style(root).cloned().unwrap_or_default();
            style.direction = Some(direction);
            let _ = inner.engine.set_style(&inner.tree, root, style);
        });
        let fragment = self.inner.fragments.root_of(root);
        let owned = self.inner.owned(scope);
        let _owner = scope::enter(owned);
        let mut cx = Cx {
            inner: &mut self.inner,
            parent: root,
            fragment,
            scope,
            env,
        };
        cx.mount(view);
        drop(_owner);
        self.flush();
    }

    /// The root environment.
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Runs the reactions of changed state until nothing changes.
    /// Returns true when any ran. Focus bindings follow focus that a
    /// reaction moved, for example through another focus binding.
    pub fn flush(&mut self) -> bool {
        let mut ran = false;
        for _ in 0..MAX_ROUNDS {
            self.inner.sync_focus();
            let mut due = std::mem::take(&mut *self.inner.queue.borrow_mut());
            if due.is_empty() {
                break;
            }
            due.sort();
            due.dedup();
            for id in due {
                self.inner.run_reaction(id);
            }
            ran = true;
        }
        if !self.inner.queue.borrow().is_empty() {
            self.inner.queue.borrow_mut().clear();
            self.inner.report(ViewError::Reactive(ReactiveError::Cycle));
        }
        for error in self.inner.runtime.take_errors() {
            self.inner.report(ViewError::Reactive(error));
        }
        for error in super::state::take_handle_errors() {
            self.inner.report(ViewError::Reactive(error));
        }
        ran
    }

    /// The number of nodes that changed since the last call. A state
    /// change costs work for its dependents only, so this stays the same
    /// however large the tree is.
    pub fn take_dirty_count(&mut self) -> usize {
        std::mem::take(&mut self.inner.touched).len()
    }

    /// Errors since the last call.
    pub fn take_errors(&mut self) -> Vec<ViewError> {
        std::mem::take(&mut self.inner.errors)
    }

    /// The retained tree of the mounted views.
    pub fn tree(&self) -> &Tree {
        &self.inner.tree
    }

    /// The size of the root node after the last layout.
    pub fn size(&self) -> Size {
        self.inner
            .tree
            .bounds(self.inner.tree.root())
            .map_or(Size::ZERO, |bounds| bounds.size)
    }

    /// The host of the mounted controls.
    pub fn controls(&self) -> &ControlHost {
        &self.inner.controls
    }

    /// The layout engine of the tree.
    pub fn layout_engine(&self) -> &LayoutEngine {
        &self.inner.engine
    }

    /// The theme resolver, for accessibility preferences.
    pub fn themes(&self) -> &ThemeResolver {
        &self.inner.themes
    }

    /// Sets the application tokens, for example after a theme change.
    pub fn set_tokens(&mut self, tokens: DesignTokens) {
        match self
            .inner
            .themes
            .set_application(&mut self.inner.tree, tokens)
        {
            Ok(change) => {
                if change == choscorui_core::ThemeInvalidation::Layout {
                    self.inner.engine.invalidate_all();
                    self.inner.needs_layout = true;
                }
                self.inner.needs_paint = true;
            }
            Err(error) => self.inner.report(ViewError::Theme(error)),
        }
    }

    /// Applies accessibility preferences, which win over every theme
    /// layer.
    pub fn set_accessibility(&mut self, preferences: choscorui_core::AccessibilityPreferences) {
        match self
            .inner
            .themes
            .set_accessibility_preferences(&mut self.inner.tree, preferences)
        {
            Ok(_) => {
                self.inner.engine.invalidate_all();
                self.inner.needs_layout = true;
                self.inner.needs_paint = true;
            }
            Err(error) => self.inner.report(ViewError::Theme(error)),
        }
    }

    /// The recipe registry, for application-wide recipes.
    pub fn styles_mut(&mut self) -> &mut Styles {
        self.inner.needs_paint = true;
        &mut self.inner.styles
    }

    /// The tokens that `node` paints with.
    pub fn tokens(&self, node: NodeId) -> DesignTokens {
        self.inner
            .themes
            .resolve(&self.inner.tree, node)
            .unwrap_or_else(|_| self.inner.themes.application())
    }

    /// Resizes the window area.
    pub fn resize(&mut self, size: Size) {
        let root = self.inner.tree.root();
        let bounds = Rect::new(Point::new(0.0, 0.0), size);
        if self.inner.tree.bounds(root) == Some(bounds) {
            return;
        }
        if let Err(error) = self.inner.tree.set_bounds(root, bounds) {
            self.inner.report(ViewError::Tree(error));
        }
        self.inner.engine.invalidate(&self.inner.tree, root);
        self.inner.needs_layout = true;
        self.inner.needs_paint = true;
    }

    /// Lays the tree out when something changed its size or content.
    pub fn layout(&mut self) {
        if !self.inner.needs_layout {
            return;
        }
        for _ in 0..LAYOUT_PASSES {
            self.layout_pass();
            // Moved nodes publish their new bounds; the others keep theirs.
            let inner = &mut self.inner;
            inner
                .semantic_dirty
                .extend(inner.engine.placed().iter().copied());
            if !self.inner.after_layout() {
                break;
            }
        }
        self.inner.needs_layout = false;
        self.inner.needs_paint = true;
    }

    fn layout_pass(&mut self) {
        self.inner.needs_layout = false;
        self.inner.arrange_children();
        let inner = &mut self.inner;
        let root = inner.tree.root();
        let bounds = inner.tree.bounds(root).unwrap_or_default();
        let env = HostEnv::new(&inner.themes, &inner.styles, &*inner.text)
            .direction(inner.direction)
            .time(inner.time);
        let mut leaves = |_: NodeId, constraints: Constraints, _| Measured {
            size: constraints.min,
            baseline: None,
        };
        if let Err(error) = inner.controls.layout(
            &mut inner.engine,
            &mut inner.tree,
            root,
            bounds,
            &env,
            &mut leaves,
        ) {
            inner.errors.push(ViewError::Layout(error));
        }
    }

    /// Lays out if needed and paints the whole tree.
    pub fn paint(&mut self) -> DisplayList {
        self.layout();
        self.inner.needs_paint = false;
        let mut list = DisplayList::new();
        super::paint::paint(&self.inner, &mut list);
        list
    }

    /// Whether the window should draw again: changed state, layout, or
    /// paint, or a running animation.
    pub fn needs_frame(&self) -> bool {
        self.inner.needs_layout
            || self.inner.needs_paint
            || !self.inner.queue.borrow().is_empty()
            || self
                .inner
                .controls
                .animating(&self.inner.tree, self.inner.time)
    }

    /// The earliest time at which a control wants a timer.
    pub fn next_deadline(&self) -> Option<f64> {
        self.inner.controls.next_deadline(&self.inner.tree)
    }

    /// Sets the time and fires due control timers.
    pub fn advance(&mut self, time: f64) -> bool {
        self.inner.time = time;
        self.inner.controls.set_time(time);
        let fired = self.inner.controls.advance(&mut self.inner.tree, time);
        if fired || self.inner.controls.animating(&self.inner.tree, time) {
            self.inner.needs_paint = true;
        }
        self.after_input();
        fired
    }

    /// Delivers control messages to their handlers, then runs reactions.
    fn after_input(&mut self) {
        // A handler can make a control emit, as a field label toggles its
        // checkbox. Those messages go out in a next round. The bound keeps
        // handlers that answer each other from holding the window.
        for _ in 0..MESSAGE_ROUNDS {
            let messages = self.inner.controls.take_messages();
            if messages.is_empty() {
                break;
            }
            for (node, message) in messages {
                let (handlers, internal) = self
                    .inner
                    .nodes
                    .get(&node)
                    .map(|data| (data.handlers.clone(), data.internal.clone()))
                    .unwrap_or_default();
                if let Some(internal) = internal {
                    internal(&mut self.inner, &message);
                }
                for handler in handlers {
                    handler(&message);
                }
            }
        }
        let open = self.inner.menu_press.borrow_mut().take();
        if let Some(open) = open {
            open();
        }
        self.inner.sync_control_dirt();
        self.flush();
    }

    /// Routes a pointer event to the controls.
    pub fn pointer(&mut self, event: PointerEvent, modifiers: ShortcutModifiers) {
        self.modifiers = modifiers;
        self.inner.controls.set_modifiers(modifiers);
        self.inner
            .controls
            .dispatch_pointer(&mut self.inner.tree, event);
        self.inner.needs_paint = true;
        self.after_input();
    }

    /// Routes a key event: a declared shortcut first, then the focused
    /// control, then Tab moves focus. Returns true when handled.
    pub fn key(&mut self, event: KeyEvent, modifiers: ShortcutModifiers) -> bool {
        self.modifiers = modifiers;
        self.inner.controls.set_modifiers(modifiers);
        if let KeyEvent::Down { key, .. } = event
            && let Some(action) = self.shortcut(key, modifiers)
        {
            action();
            self.after_input();
            return true;
        }
        let mut handled = self
            .inner
            .controls
            .dispatch_key(&mut self.inner.tree, event);
        if !handled && let KeyEvent::Down { key: Key::Tab, .. } = event {
            self.inner.tree.set_input_modality(InputModality::Keyboard);
            self.inner
                .controls
                .focus_next(&mut self.inner.tree, modifiers.shift);
            self.inner.semantic_all = true;
            handled = true;
        }
        self.inner.needs_paint = true;
        self.after_input();
        handled
    }

    /// The action of the shortcut for `key`, preferring the one declared
    /// closest to the focus.
    fn shortcut(&self, key: Key, modifiers: ShortcutModifiers) -> Option<Rc<dyn Fn()>> {
        let focus = self.inner.tree.focus();
        let matching = self
            .inner
            .shortcuts
            .iter()
            .filter(|entry| entry.key == key && entry.modifiers == modifiers);
        let mut best: Option<(&ShortcutEntry, usize)> = None;
        for entry in matching {
            let depth = focus
                .filter(|focus| self.inner.tree.is_within(*focus, entry.node))
                .map_or(usize::MAX, |focus| self.depth_between(focus, entry.node));
            if best.is_none_or(|(_, best_depth)| depth < best_depth) {
                best = Some((entry, depth));
            }
        }
        best.map(|(entry, _)| entry.action.clone())
    }

    fn depth_between(&self, node: NodeId, ancestor: NodeId) -> usize {
        let mut depth = 0;
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return depth;
            }
            depth += 1;
            current = self.inner.tree.parent(id);
        }
        usize::MAX - 1
    }

    /// Delivers committed text to the focused control.
    pub fn text(&mut self, text: &str) -> bool {
        let handled = self
            .inner
            .controls
            .dispatch_text(&mut self.inner.tree, text);
        self.inner.needs_paint = true;
        self.after_input();
        handled
    }

    /// Performs an assistive action on `node`.
    pub fn perform(&mut self, node: NodeId, action: SemanticAction, value: Option<&str>) -> bool {
        let opener = self
            .inner
            .nodes
            .get(&node)
            .and_then(|data| data.show_menu.clone());
        let handled = if action == SemanticAction::Focus && !self.inner.controls.contains(node) {
            self.focus(Some(node));
            true
        } else if let Some(open) = opener
            .filter(|_| action == SemanticAction::ShowMenu && self.inner.controls.is_enabled(node))
        {
            open(&self.inner);
            true
        } else {
            self.inner
                .controls
                .perform(&mut self.inner.tree, node, action, value)
        };
        self.inner.needs_paint = true;
        self.after_input();
        handled
    }

    /// Moves keyboard focus to `node`.
    pub fn focus(&mut self, node: Option<NodeId>) {
        self.inner.controls.move_focus(&mut self.inner.tree, node);
        self.inner.needs_paint = true;
        self.inner.semantic_all = true;
        self.after_input();
    }

    /// Moves keyboard focus forward or back.
    pub fn focus_next(&mut self, reverse: bool) -> Option<NodeId> {
        self.inner.tree.set_input_modality(InputModality::Keyboard);
        let next = self
            .inner
            .controls
            .focus_next(&mut self.inner.tree, reverse);
        self.inner.needs_paint = true;
        self.inner.semantic_all = true;
        self.after_input();
        next
    }

    /// Updates the semantic tree and returns its snapshot with the
    /// changes since the last call.
    pub fn semantics(&mut self) -> (Vec<SemanticEntry>, Vec<SemanticChange>) {
        self.layout();
        let mut changes = Vec::new();
        let removed = self.inner.semantics.prune(&self.inner.tree);
        if !removed.is_empty() {
            changes.push(SemanticChange::Tree);
        }
        self.sync_tooltip_node();
        let all = std::mem::take(&mut self.inner.semantic_all);
        let mut candidates: Vec<NodeId> = if all {
            let mut nodes: Vec<NodeId> = self.inner.nodes.keys().copied().collect();
            nodes.sort();
            nodes
        } else {
            std::mem::take(&mut self.inner.semantic_dirty)
                .into_iter()
                .collect()
        };
        if all {
            self.inner.semantic_dirty.clear();
        }
        candidates.retain(|node| self.inner.tree.contains(*node));
        for node in candidates {
            let _ = self.inner.tree.take_dirty(node);
            match self.semantic_node(node) {
                Some(semantic) => {
                    match self.inner.semantics.set(node, semantic, &self.inner.tree) {
                        Ok(SemanticChange::None) => {}
                        Ok(change) => changes.push(change),
                        Err(error) => self.inner.report(ViewError::Semantics(error)),
                    }
                }
                None => {
                    if self.inner.semantics.remove(node) {
                        changes.push(SemanticChange::Tree);
                    }
                }
            }
        }
        if let Some(change) = self.inner.semantics.sync_focus(&self.inner.tree) {
            changes.push(change);
        }
        (self.inner.semantics.snapshot(&self.inner.tree), changes)
    }

    /// Marks the nodes that publish the shown tooltip, before and now,
    /// when it changed.
    fn sync_tooltip_node(&mut self) {
        let inner = &mut self.inner;
        let shown = inner
            .tooltips
            .borrow()
            .showing()
            .and_then(|wrapper| inner.nodes.get(&wrapper))
            .and_then(|data| data.bubble);
        if shown == inner.tooltip_node {
            return;
        }
        let before = std::mem::replace(&mut inner.tooltip_node, shown);
        inner.semantic_dirty.extend(before.into_iter().chain(shown));
    }

    /// The semantic node that `node` publishes, if any.
    fn semantic_node(&self, node: NodeId) -> Option<SemanticNode> {
        let data = self.inner.nodes.get(&node)?;
        if data.silent || data.access.hidden || self.hidden_by_ancestor(node) {
            return None;
        }
        if let Some((wrapper, view)) = data.describes {
            return self.inner.tooltips.borrow().semantics(wrapper, view);
        }
        let mut semantic = match self.inner.controls.get(node) {
            Some(control) => {
                let mut semantic = control.semantics();
                if !self.inner.controls.is_enabled(node) {
                    semantic.state.disabled = true;
                    semantic.actions.clear();
                }
                semantic
            }
            None if data.access.is_set() || data.show_menu.is_some() => SemanticNode {
                role: SemanticRole::Group,
                name: String::new(),
                description: String::new(),
                value: None,
                state: choscorui_core::SemanticState::default(),
                relations: Vec::new(),
                actions: Vec::new(),
                text: None,
            },
            None => return None,
        };
        if let Some(label) = &data.access.label {
            semantic.name.clone_from(label);
        }
        let access = &data.access;
        if let Some(hint) = access
            .hint
            .as_ref()
            .or(access.tooltip.as_ref())
            .or(access.field.as_ref())
        {
            semantic.description.clone_from(hint);
        }
        semantic.state.invalid |= access.invalid;
        for relation in &access.relations {
            if !semantic.relations.contains(relation) {
                semantic.relations.push(*relation);
            }
        }
        if data.show_menu.is_some()
            && !semantic.state.disabled
            && !semantic.actions.contains(&SemanticAction::ShowMenu)
        {
            semantic.actions.push(SemanticAction::ShowMenu);
        }
        if let Some(value) = &data.access.value {
            semantic.value = Some(value.clone());
        }
        Some(semantic)
    }

    fn hidden_by_ancestor(&self, node: NodeId) -> bool {
        let mut current = self.inner.tree.parent(node);
        while let Some(id) = current {
            if self
                .inner
                .nodes
                .get(&id)
                .is_some_and(|data| data.access.hidden)
            {
                return true;
            }
            current = self.inner.tree.parent(id);
        }
        false
    }

    /// The first published semantic node named `name`, for tests and
    /// scripted checks.
    pub fn find(&mut self, name: &str) -> Option<NodeId> {
        let (snapshot, _) = self.semantics();
        snapshot
            .into_iter()
            .find(|entry| entry.node.name == name)
            .map(|entry| entry.id)
    }

    /// The published semantic node of `node`.
    pub fn semantic(&mut self, node: NodeId) -> Option<SemanticNode> {
        let (snapshot, _) = self.semantics();
        snapshot
            .into_iter()
            .find(|entry| entry.id == node)
            .map(|entry| entry.node)
    }

    /// Ends pointer capture and hover, for example when the window loses
    /// focus.
    pub fn cancel_interaction(&mut self) {
        let _ = self.inner.tree.set_pointer_capture(None);
        self.inner.needs_paint = true;
    }
}
