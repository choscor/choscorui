//! Toasts. The `toaster` modifier puts a toast queue in the environment
//! and mounts the controls' toast stack above the window content. Any
//! descendant posts to the queue through [`Env::toasts`] or the `toast`
//! modifier. The stack keeps the controls' timers, so toasts close by
//! themselves and pause while the pointer rests on them.

use super::configured::Configured;
use super::env::Env;
use super::host::{Cx, ViewError};
use super::state::{Binding, Live};
use super::text::TextContent;
use super::widgets::text;
use crate::reactive::Runtime;
use choscorui_controls::{ControlKind, Toast, ToastPosition, Toaster, Toasts};
use std::rc::Rc;

/// Makes a toast in the environment of the view that posts it.
pub(crate) type MakeToast = Rc<dyn Fn(&Env) -> Toast>;

/// The settings of a `toaster` modifier.
pub(crate) struct ToasterSpec {
    toasts: Toasts,
    position: ToastPosition,
    name: Live<TextContent>,
    close: Live<TextContent>,
}

impl ToasterSpec {
    /// A toaster with a new, empty queue.
    pub(crate) fn new(
        position: ToastPosition,
        name: Live<TextContent>,
        close: Live<TextContent>,
    ) -> Self {
        Self {
            toasts: Toasts::new(),
            position,
            name,
            close,
        }
    }

    /// The queue that descendants post to.
    pub(crate) fn toasts(&self) -> Toasts {
        self.toasts.clone()
    }
}

impl Env {
    /// The toast queue of the nearest `toaster` above, if any. A toast
    /// pushed to it shows at the next host time step.
    pub fn toasts(&self) -> Option<Toasts> {
        self.value::<Toasts>().cloned()
    }
}

/// Mounts the toast stack of `spec` above the window content, until the
/// view unmounts.
pub(super) fn mount_toaster(cx: &mut Cx<'_>, env: &Env, spec: ToasterSpec) {
    let root = cx.inner.tree.root();
    let root_fragment = cx.inner.fragments.root_of(root);
    let fragment = cx.inner.fragments.child(root_fragment);
    let ToasterSpec {
        toasts,
        position,
        name,
        close,
    } = spec;
    let stack = Configured::custom(ControlKind::Toast, move |env| {
        Toaster::new(text(&name, env), toasts.clone(), text(&close, env)).with_position(position)
    });
    let mut stack_cx = Cx {
        inner: &mut *cx.inner,
        parent: root,
        fragment,
        scope: cx.scope,
        env: env.clone(),
    };
    stack_cx.mount(stack);
    cx.inner.on_dispose(
        cx.scope,
        Box::new(move |inner| {
            let nodes = inner.fragments.clear(fragment);
            inner.remove_nodes(&nodes);
        }),
    );
}

/// Posts the toast of `make` each time `presented` becomes true, and sets
/// `presented` back to false.
pub(super) fn post(cx: &mut Cx<'_>, env: &Env, presented: Binding<bool>, make: MakeToast) {
    let env = env.clone();
    cx.react(move |inner| {
        if !presented.try_get().unwrap_or(false) {
            return;
        }
        Runtime::untracked(|| {
            match env.toasts() {
                Some(toasts) => {
                    toasts.push(make(&env));
                }
                None => inner.report(ViewError::NoToaster),
            }
            presented.set(false);
        });
    });
}
