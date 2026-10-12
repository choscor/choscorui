//! Structural change: `Dynamic` mounts the view that a closure returns
//! again whenever the state the closure read changes, and `ForEach`
//! reconciles keyed children. Both keep their nodes at their place among
//! their siblings.

use super::View;
use super::env::Env;
use super::fragment::FragmentId;
use super::host::{Cx, Inner, ScopeId, ViewError};
use super::scope;
use super::state::{IntoLive, Live};
use crate::reactive::Runtime;
use choscorui_core::NodeId;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

/// Mounts `view` in `fragment` under a new scope inside `owner`.
fn mount_in(
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
    fn mount(self, cx: &mut Cx<'_>) {
        let fragment = cx.inner.fragments.child(cx.fragment);
        let owner = cx.scope;
        let env = cx.env.clone();
        let current: Rc<Cell<Option<ScopeId>>> = Rc::default();
        let choose = self.choose;
        cx.react(move |inner| {
            let view = choose();
            Runtime::untracked(|| {
                if let Some(scope) = current.take() {
                    inner.dispose_scope(scope);
                }
                let nodes = inner.fragments.clear(fragment);
                inner.remove_nodes(&nodes);
                // A modifier may have moved the fragment under a wrapper.
                let Some(parent) = inner.fragments.parent(fragment) else {
                    return;
                };
                let scope = mount_in(inner, owner, parent, fragment, env.clone(), view);
                current.set(Some(scope));
            });
        });
    }
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
    fn mount(self, cx: &mut Cx<'_>) {
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
                let mut reusable: HashMap<K, Entry<K, T>> = old
                    .into_iter()
                    .map(|entry| (entry.key.clone(), entry))
                    .collect();
                let mut seen = HashSet::new();
                let mut next = Vec::with_capacity(items.len());
                for item in items {
                    let item_key = key(&item);
                    if !seen.insert(item_key.clone()) {
                        inner.report(ViewError::DuplicateKey(format!("{item_key:?}")));
                        continue;
                    }
                    match reusable.remove(&item_key) {
                        Some(entry) if entry.item == item => next.push(entry),
                        stale => {
                            if let Some(entry) = stale {
                                inner.dispose_scope(entry.scope);
                                let nodes = inner.fragments.remove(entry.fragment);
                                inner.remove_nodes(&nodes);
                            }
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
                                key: item_key,
                                item,
                                fragment: item_fragment,
                                scope,
                            });
                        }
                    }
                }
                for (_, entry) in reusable {
                    inner.dispose_scope(entry.scope);
                    let nodes = inner.fragments.remove(entry.fragment);
                    inner.remove_nodes(&nodes);
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
fn reorder_nodes(inner: &mut Inner, parent: NodeId, fragment: FragmentId) {
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
