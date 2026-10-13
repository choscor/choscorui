//! Structural change: `Dynamic` mounts the view that a closure returns
//! again whenever the state the closure read changes, and `ForEach`
//! reconciles keyed children. Both keep their nodes at their place among
//! their siblings.
//!
//! `ForEach` compares its old and new item keys with [`plan_keyed`], the
//! keyed diff of the `Reconciler`. An item can make many nodes or none, so
//! the plan works on the item fragments and not on single nodes.
//!
//! `Dynamic` and `Branch` are the scopes of `Option`, `Either`, and
//! `AnyView`. Each is a keyed scope with one child, which goes through
//! [`plan_keyed`] too. A `Branch` child's key is the value of its key
//! closure, so a run that gives the same key keeps the child. A view value
//! has no identity that a run can compare, so the key of a `Dynamic` child
//! is the run, and each run replaces the child.

use super::View;
use super::env::Env;
use super::fragment::FragmentId;
use super::host::{Cx, Inner, ScopeId, ViewError};
use super::scope;
use super::state::{IntoLive, Live};
use crate::reactive::Runtime;
use crate::reconcile::plan_keyed;
use choscorui_core::NodeId;
use std::cell::{Cell, RefCell};
use std::hash::Hash;
use std::rc::Rc;

/// Mounts `view` in `fragment` under a new scope inside `owner`.
pub(super) fn mount_in(
    inner: &mut Inner,
    owner: ScopeId,
    parent: NodeId,
    fragment: FragmentId,
    env: Env,
    view: impl View,
) -> ScopeId {
    let scope = inner.child_scope(owner);
    let _owner = scope::enter(inner.owned(scope));
    let mut cx = Cx {
        inner,
        parent,
        fragment,
        scope,
        env,
    };
    cx.mount(view);
    scope
}

/// A view that a closure chooses. The closure runs again, and its view
/// mounts again in place of the old one, whenever state that the closure
/// read changes: `Dynamic::new(move || show.get().then(|| Text::new("Hi")))`.
/// The closure should read only what decides the structure; the views it
/// returns update their own live properties.
pub struct Dynamic<F> {
    choose: F,
}

impl<F, V> Dynamic<F>
where
    F: Fn() -> V + 'static,
    V: View,
{
    pub fn new(choose: F) -> Self {
        Self { choose }
    }
}

impl<F, V> View for Dynamic<F>
where
    F: Fn() -> V + 'static,
    V: View,
{
    fn mount(self, cx: &mut Cx) {
        let choose = self.choose;
        let runs = Cell::new(0_u64);
        mount_branch(cx, move || {
            let view = choose();
            runs.set(runs.get() + 1);
            (runs.get(), Box::new(move || view) as Box<dyn FnOnce() -> V>)
        });
    }
}

/// A view that a key chooses: `Branch::new(move || tab.get(), |tab| page(*tab))`.
/// The key closure runs again whenever state that it read changes. The
/// view closure runs only when the key changes, and its view replaces the
/// old one; while the key stays, the mounted view keeps its nodes, focus,
/// semantics, and state.
pub struct Branch<KF, VF> {
    key: KF,
    view: VF,
}

impl<K, V, KF, VF> Branch<KF, VF>
where
    K: Clone + Eq + Hash + 'static,
    V: View,
    KF: Fn() -> K + 'static,
    VF: Fn(&K) -> V + 'static,
{
    pub fn new(key: KF, view: VF) -> Self {
        Self { key, view }
    }
}

impl<K, V, KF, VF> View for Branch<KF, VF>
where
    K: Clone + Eq + Hash + 'static,
    V: View,
    KF: Fn() -> K + 'static,
    VF: Fn(&K) -> V + 'static,
{
    fn mount(self, cx: &mut Cx) {
        let (key, view) = (self.key, Rc::new(self.view));
        mount_branch(cx, move || {
            let key = key();
            let view = Rc::clone(&view);
            let chosen = key.clone();
            (
                key,
                Box::new(move || view(&chosen)) as Box<dyn FnOnce() -> V>,
            )
        });
    }
}

/// The mounted child of a [`Dynamic`] or [`Branch`] scope.
struct Child<K> {
    key: K,
    scope: ScopeId,
}

/// Mounts the one keyed child that `next` chooses, and reconciles it with
/// [`plan_keyed`] whenever state that `next` read changes. The child view
/// is made only when the plan creates the child.
fn mount_branch<K, V>(cx: &mut Cx, next: impl Fn() -> (K, Box<dyn FnOnce() -> V>) + 'static)
where
    K: Clone + Eq + Hash + 'static,
    V: View,
{
    let fragment = cx.inner.fragments.child(cx.fragment);
    let owner = cx.scope;
    let env = cx.env.clone();
    let current: Rc<RefCell<Option<Child<K>>>> = Rc::default();
    cx.react(move |inner| {
        let (key, make) = next();
        Runtime::untracked(|| {
            let old = current.borrow_mut().take();
            let old_keys: Vec<K> = old.iter().map(|child| child.key.clone()).collect();
            let plan = plan_keyed(&old_keys, std::slice::from_ref(&key), |_, _| true);
            if plan.removed.is_empty()
                && let Some(child) = old
            {
                *current.borrow_mut() = Some(child);
                return;
            }
            if let Some(child) = old {
                inner.dispose_scope(child.scope);
            }
            let nodes = inner.fragments.clear(fragment);
            inner.remove_nodes(&nodes);
            // A modifier may have moved the fragment under a wrapper.
            let Some(parent) = inner.fragments.parent(fragment) else {
                return;
            };
            let scope = mount_in(inner, owner, parent, fragment, env.clone(), make());
            *current.borrow_mut() = Some(Child { key, scope });
        });
    });
}

/// One mounted item of a [`ForEach`].
struct Entry<K, T> {
    key: K,
    item: T,
    fragment: FragmentId,
    scope: ScopeId,
}

/// Keyed children from a collection. When the collection changes, items
/// whose key survives keep their nodes, focus, and semantics; new keys
/// mount, removed keys unmount, and the nodes follow the new order. An
/// item whose value changed under the same key mounts again.
pub struct ForEach<T, K, V> {
    items: Live<Vec<T>>,
    key: Rc<dyn Fn(&T) -> K>,
    view: Rc<dyn Fn(&T) -> V>,
}

impl<T, K, V> ForEach<T, K, V>
where
    T: Clone + PartialEq + 'static,
    K: Clone + Eq + Hash + std::fmt::Debug + 'static,
    V: View,
{
    pub fn new<M>(
        items: impl IntoLive<Vec<T>, M>,
        key: impl Fn(&T) -> K + 'static,
        view: impl Fn(&T) -> V + 'static,
    ) -> Self {
        Self {
            items: items.into_live(),
            key: Rc::new(key),
            view: Rc::new(view),
        }
    }
}

impl<T, K, V> View for ForEach<T, K, V>
where
    T: Clone + PartialEq + 'static,
    K: Clone + Eq + Hash + std::fmt::Debug + 'static,
    V: View,
{
    fn mount(self, cx: &mut Cx) {
        let fragment = cx.inner.fragments.child(cx.fragment);
        let owner = cx.scope;
        let env = cx.env.clone();
        let entries: Rc<RefCell<Vec<Entry<K, T>>>> = Rc::default();
        let (items, key, view) = (self.items, self.key, self.view);
        cx.react(move |inner| {
            let items = items.get();
            // A modifier may have moved the fragment under a wrapper.
            let Some(parent) = inner.fragments.parent(fragment) else {
                return;
            };
            Runtime::untracked(|| {
                let old = std::mem::take(&mut *entries.borrow_mut());
                let old_keys: Vec<K> = old.iter().map(|entry| entry.key.clone()).collect();
                let new_keys: Vec<K> = items.iter().map(|item| key(item)).collect();
                // A kept key whose value changed mounts again.
                let plan = plan_keyed(&old_keys, &new_keys, |old_index, new_index| {
                    old[old_index].item == items[new_index]
                });
                for duplicate in &plan.duplicates {
                    inner.report(ViewError::DuplicateKey(format!("{duplicate:?}")));
                }
                let mut old: Vec<Option<Entry<K, T>>> = old.into_iter().map(Some).collect();
                for index in &plan.removed {
                    if let Some(entry) = old[*index].take() {
                        inner.dispose_scope(entry.scope);
                        let nodes = inner.fragments.remove(entry.fragment);
                        inner.remove_nodes(&nodes);
                    }
                }
                let mut items: Vec<Option<T>> = items.into_iter().map(Some).collect();
                let mut next = Vec::with_capacity(plan.children.len());
                for child in plan.children {
                    if let Some(entry) = child.reuse.and_then(|index| old[index].take()) {
                        next.push(entry);
                        continue;
                    }
                    let Some(item) = items[child.index].take() else {
                        continue;
                    };
                    let item_fragment = inner.fragments.child(fragment);
                    let scope = mount_in(
                        inner,
                        owner,
                        parent,
                        item_fragment,
                        env.clone(),
                        view(&item),
                    );
                    next.push(Entry {
                        key: child.key,
                        item,
                        fragment: item_fragment,
                        scope,
                    });
                }
                let order: Vec<FragmentId> = next.iter().map(|entry| entry.fragment).collect();
                inner.fragments.reorder(fragment, &order);
                reorder_nodes(inner, parent, fragment);
                *entries.borrow_mut() = next;
            });
        });
    }
}

/// Moves the fragment's nodes so that the tree follows the fragment.
pub(super) fn reorder_nodes(inner: &mut Inner, parent: NodeId, fragment: FragmentId) {
    let base = inner.fragments.offset(fragment);
    let nodes = inner.fragments.nodes(fragment);
    for (index, node) in nodes.into_iter().enumerate() {
        let at = base + index;
        let placed = inner
            .tree
            .children(parent)
            .and_then(|children| children.get(at))
            == Some(&node);
        if !placed {
            match inner.tree.place_child(parent, node, at) {
                Ok(()) => {
                    inner.touch(parent, choscorui_core::Dirty::LAYOUT);
                    inner.children_changed(parent);
                }
                Err(error) => inner.report(ViewError::Tree(error)),
            }
        }
    }
}
