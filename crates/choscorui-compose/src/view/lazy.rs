//! Lazy stacks: `LazyVStack` and `LazyHStack` mount only the items that
//! show. Each item has one fixed extent along the stack, so the visible
//! items follow from the scroll position alone (core `UniformStack`), and
//! layout, paint, and input cost the same for any item count.

use super::dynamic::{mount_in, reorder_nodes};
use super::env::{Env, Metric};
use super::fragment::FragmentId;
use super::host::{Cx, Inner, ScopeId, ViewError};
use super::layout::{DEFAULT_SPACING, HorizontalAlignment, VerticalAlignment, parent_tokens};
use super::state::{IntoLive, Live};
use super::{AnyView, View};
use crate::reactive::Runtime;
use choscorui_core::{
    Axis, EngineError, ItemAlign, LayoutDirection, LayoutKind, LayoutStyle, Length, MainAlign,
    NodeId, Rect, UniformStack,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;

/// Makes the view of one item from its index.
type ItemFn = Rc<dyn Fn(usize) -> AnyView>;

/// Work that runs after each layout pass. It returns true when it changed
/// the tree, so that layout has to run again.
pub(crate) type LazyHook = Rc<dyn Fn(&mut Inner) -> bool>;

/// Items top to bottom, mounted only while they show in the enclosing
/// scroll view or the window. Every item is `item_extent` high:
/// `LazyVStack::new(move || rows.get(), 32.0, |index| Text::new(format!("Row {index}")))`.
///
/// An item that scrolls out of view unmounts, and its state ends; an
/// item that scrolls in mounts again. The item view runs when the item
/// mounts, so data that changes belongs in live properties.
pub struct LazyVStack {
    lazy: Lazy,
    alignment: HorizontalAlignment,
}

impl LazyVStack {
    /// `count` items, each `item_extent` high, from `view`.
    pub fn new<M, V: View>(
        count: impl IntoLive<usize, M>,
        item_extent: impl Into<Metric>,
        view: impl Fn(usize) -> V + 'static,
    ) -> Self {
        Self {
            lazy: Lazy::new(count.into_live(), item_extent.into(), view),
            alignment: HorizontalAlignment::Center,
        }
    }

    /// The distance between items.
    #[must_use]
    pub fn spacing(mut self, spacing: impl Into<Metric>) -> Self {
        self.lazy.spacing = spacing.into();
        self
    }

    /// Where items sit across the stack.
    #[must_use]
    pub fn alignment(mut self, alignment: HorizontalAlignment) -> Self {
        self.alignment = alignment;
        self
    }
}

impl View for LazyVStack {
    fn mount(self, cx: &mut Cx) {
        self.lazy.mount(cx, Axis::Vertical, self.alignment.item());
    }
}

/// Items from the leading edge to the trailing edge, mounted only while
/// they show; mirrored in right-to-left layouts. Every item is
/// `item_extent` wide. [`LazyVStack`] gives the other rules.
pub struct LazyHStack {
    lazy: Lazy,
    alignment: VerticalAlignment,
}

impl LazyHStack {
    /// `count` items, each `item_extent` wide, from `view`.
    pub fn new<M, V: View>(
        count: impl IntoLive<usize, M>,
        item_extent: impl Into<Metric>,
        view: impl Fn(usize) -> V + 'static,
    ) -> Self {
        Self {
            lazy: Lazy::new(count.into_live(), item_extent.into(), view),
            alignment: VerticalAlignment::Center,
        }
    }

    #[must_use]
    pub fn spacing(mut self, spacing: impl Into<Metric>) -> Self {
        self.lazy.spacing = spacing.into();
        self
    }

    #[must_use]
    pub fn alignment(mut self, alignment: VerticalAlignment) -> Self {
        self.alignment = alignment;
        self
    }
}

impl View for LazyHStack {
    fn mount(self, cx: &mut Cx) {
        self.lazy.mount(cx, Axis::Horizontal, self.alignment.item());
    }
}

/// What both lazy stacks hold before they mount.
struct Lazy {
    count: Live<usize>,
    extent: Metric,
    spacing: Metric,
    view: ItemFn,
}

impl Lazy {
    fn new<V: View>(
        count: Live<usize>,
        extent: Metric,
        view: impl Fn(usize) -> V + 'static,
    ) -> Self {
        Self {
            count,
            extent,
            spacing: DEFAULT_SPACING.into(),
            view: Rc::new(move |index| view(index).into_any()),
        }
    }

    /// Mounts a stack node with one track node inside. The track is as
    /// long as all items, and its leading padding puts the first mounted
    /// item at its place.
    fn mount(self, cx: &mut Cx, axis: Axis, cross: ItemAlign) {
        let tokens = parent_tokens(cx);
        let extent = self.extent.resolve(&tokens);
        let spacing = self.spacing.resolve(&tokens);
        let Ok(empty) = UniformStack::new(0, extent, spacing) else {
            cx.inner
                .report(ViewError::Layout(EngineError::InvalidStyle));
            return;
        };
        let outer = cx.node(stack_style(axis, 0.0, cross));
        let env = cx.env.clone();
        let track = cx
            .within(outer, env.clone())
            .node(stack_style(axis, spacing, cross));
        let fragment = cx.inner.fragments.root_of(track);
        let state = Rc::new(RefCell::new(Window {
            stack: empty,
            shown: None,
            items: BTreeMap::new(),
        }));
        let mounter = Rc::new(Mounter {
            owner: cx.scope,
            env,
            view: self.view,
            track,
            fragment,
            axis,
            cross,
        });
        let hook_state = state.clone();
        let hook_mounter = mounter.clone();
        let hook: LazyHook = Rc::new(move |inner| update(inner, &hook_mounter, &hook_state));
        cx.inner.lazies.insert(track, hook);
        cx.bind(self.count, move |inner, count| {
            set_count(inner, &mounter, &state, count);
        });
    }
}

/// The style of a stack node along `axis`.
fn stack_style(axis: Axis, spacing: f64, cross: ItemAlign) -> LayoutStyle {
    LayoutStyle {
        kind: LayoutKind::Stack {
            axis,
            spacing,
            main_align: MainAlign::Start,
            cross_align: cross,
        },
        ..LayoutStyle::default()
    }
}

/// What a mounted lazy stack needs to mount an item.
struct Mounter {
    owner: ScopeId,
    env: Env,
    view: ItemFn,
    track: NodeId,
    fragment: FragmentId,
    axis: Axis,
    cross: ItemAlign,
}

/// One mounted item.
struct Item {
    fragment: FragmentId,
    scope: ScopeId,
}

/// The items of a mounted lazy stack and the window that they fill.
struct Window {
    stack: UniformStack,
    /// The indices of the mounted items; `None` makes the next layout
    /// work out the window again.
    shown: Option<Range<usize>>,
    items: BTreeMap<usize, Item>,
}

/// Applies a new item count: the track takes the new length, and items
/// past the end unmount at once.
fn set_count(inner: &mut Inner, mounter: &Mounter, state: &RefCell<Window>, count: usize) {
    let stale = {
        let mut window = state.borrow_mut();
        let Ok(stack) = UniformStack::new(count, window.stack.extent(), window.stack.spacing())
        else {
            return;
        };
        window.stack = stack;
        window.shown = None;
        window.items.split_off(&count)
    };
    for item in stale.into_values() {
        unmount(inner, item);
    }
    let mut style = inner
        .engine
        .style(mounter.track)
        .cloned()
        .unwrap_or_default();
    let length = Length::Points(state.borrow().stack.length());
    match mounter.axis {
        Axis::Vertical => style.height = length,
        Axis::Horizontal => style.width = length,
    }
    inner.set_style(mounter.track, style);
}

fn unmount(inner: &mut Inner, item: Item) {
    inner.dispose_scope(item.scope);
    let nodes = inner.fragments.remove(item.fragment);
    inner.remove_nodes(&nodes);
}

/// Mounts the items that show after a layout pass and unmounts the ones
/// that left the view. Returns true when the items changed.
fn update(inner: &mut Inner, mounter: &Mounter, state: &RefCell<Window>) -> bool {
    let track = mounter.track;
    if !inner.tree.contains(track) {
        return false;
    }
    let rtl = inner.direction == LayoutDirection::Rtl;
    let stack = state.borrow().stack;
    let window = match visible_span(inner, track, mounter.axis, rtl) {
        Some((start, end)) => stack.window(start, end),
        None => 0..0,
    };
    if state.borrow().shown.as_ref() == Some(&window) {
        return false;
    }
    let stale = {
        let mut current = state.borrow_mut();
        let mut kept = current.items.split_off(&window.start);
        let after = kept.split_off(&window.end);
        let before = std::mem::replace(&mut current.items, kept);
        before
            .into_values()
            .chain(after.into_values())
            .collect::<Vec<_>>()
    };
    for item in stale {
        unmount(inner, item);
    }
    for index in window.clone() {
        if state.borrow().items.contains_key(&index) {
            continue;
        }
        let fragment = inner.fragments.child(mounter.fragment);
        let cell = Cell {
            axis: mounter.axis,
            extent: stack.extent(),
            cross: mounter.cross,
            view: Runtime::untracked(|| (mounter.view)(index)),
        };
        let scope = mount_in(
            inner,
            mounter.owner,
            track,
            fragment,
            mounter.env.clone(),
            cell,
        );
        state
            .borrow_mut()
            .items
            .insert(index, Item { fragment, scope });
    }
    let order: Vec<FragmentId> = state
        .borrow()
        .items
        .values()
        .map(|item| item.fragment)
        .collect();
    inner.fragments.reorder(mounter.fragment, &order);
    reorder_nodes(inner, track, mounter.fragment);
    let lead = stack.offset(window.start);
    let mut style = inner.engine.style(track).cloned().unwrap_or_default();
    match mounter.axis {
        Axis::Vertical => style.padding.top = lead,
        // The first item sits at the leading edge.
        Axis::Horizontal if rtl => style.padding.right = lead,
        Axis::Horizontal => style.padding.left = lead,
    }
    inner.set_style(track, style);
    state.borrow_mut().shown = Some(window);
    true
}

/// The part of `track` that shows, along `axis`, in points from its
/// leading edge: inside every enclosing scroll view and the window.
/// Across the axis the track only has to touch them, as a stack with no
/// items mounted has no extent there.
fn visible_span(inner: &Inner, track: NodeId, axis: Axis, rtl: bool) -> Option<(f64, f64)> {
    let spans = |rect: Rect| match axis {
        Axis::Vertical => ((rect.min_y(), rect.max_y()), (rect.min_x(), rect.max_x())),
        Axis::Horizontal => ((rect.min_x(), rect.max_x()), (rect.min_y(), rect.max_y())),
    };
    let (own, own_cross) = spans(inner.tree.bounds(track)?);
    let (mut low, mut high) = own;
    let mut clips = vec![inner.tree.root()];
    let mut current = inner.tree.parent(track);
    while let Some(node) = current {
        if inner.scrolls.contains(&node) {
            clips.push(node);
        }
        current = inner.tree.parent(node);
    }
    for node in clips {
        let (main, cross) = spans(inner.tree.bounds(node)?);
        if own_cross.0 > cross.1 || own_cross.1 < cross.0 {
            return None;
        }
        low = low.max(main.0);
        high = high.min(main.1);
    }
    if low >= high {
        return None;
    }
    Some(if axis == Axis::Horizontal && rtl {
        (own.1 - high, own.1 - low)
    } else {
        (low - own.0, high - own.0)
    })
}

/// The node of one item: a stack of the item's extent along the axis, so
/// that each item takes exactly its slot.
struct Cell {
    axis: Axis,
    extent: f64,
    cross: ItemAlign,
    view: AnyView,
}

impl View for Cell {
    fn mount(self, cx: &mut Cx) {
        let mut style = stack_style(self.axis, 0.0, self.cross);
        match self.axis {
            Axis::Vertical => style.height = Length::Points(self.extent),
            Axis::Horizontal => style.width = Length::Points(self.extent),
        }
        let node = cx.node(style);
        let env = cx.env.clone();
        cx.within(node, env).mount(self.view);
    }
}
