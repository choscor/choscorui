//! Container views that hold their content as children. The group box
//! node lowers onto the controls' group layout: its first child is the
//! hosted card, which paints the frame and the title, and the members
//! follow. The stack node holds the hosted stack control, and the view of
//! its current page fills it. A wrapping aspect-ratio box holds its child
//! the same way.

use super::View;
use super::configured::Configured;
use super::dynamic::Dynamic;
use super::env::Corner;
use super::host::{Cx, Flex};
use super::layout::{Alignment, parent_tokens};
use super::modifier::ViewExt;
use super::state::{IntoLive, Live};
use super::text::TextContent;
use super::widgets::{Items, Label, label, text, to_position};
use choscorui_controls as controls;
use choscorui_controls::{ControlKind, CustomControl, Slot, StyleQuery};
use choscorui_core::{Dirty, LayoutKind, LayoutStyle, NodeId, SemanticRole, SharedLayout};
use std::rc::Rc;

/// Mounts `control` with `child` as its child, which fills it. Returns
/// the node, or `None` when the control did not mount.
fn holder<T: CustomControl + 'static, V: View>(
    cx: &mut Cx,
    control: Configured<T>,
    child: V,
) -> Option<NodeId> {
    let fragment = cx.inner.fragments.child(cx.fragment);
    {
        let mut part = Cx {
            inner: &mut *cx.inner,
            parent: cx.parent,
            fragment,
            scope: cx.scope,
            env: cx.env.clone(),
        };
        part.mount(control);
    }
    let node = cx.inner.fragments.nodes(fragment).first().copied()?;
    cx.inner.set_style(
        node,
        LayoutStyle {
            kind: LayoutKind::Overlay,
            ..LayoutStyle::default()
        },
    );
    cx.inner.data(node).child_align = Some(Alignment::FILL.items());
    let env = cx.env.clone();
    cx.within(node, env).mount(child);
    cx.inner.children_changed(node);
    Some(node)
}

impl super::widgets::AspectRatio {
    /// A box named `name` with the width-to-height `ratio`, which holds
    /// `child`. The box takes the width it is offered. The child fills the
    /// box and is clipped to its corners.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let preview = AspectRatio::wrap("Preview", 16.0 / 9.0, Text::new("Video"));
    /// ```
    pub fn wrap<M, V: View>(
        name: impl IntoLive<TextContent, M>,
        ratio: f64,
        child: V,
    ) -> impl View {
        Wrap {
            control: Self::new(name, ratio),
            ratio,
            child,
        }
    }
}

/// An aspect-ratio box that holds a child view.
struct Wrap<V> {
    control: super::widgets::AspectRatio,
    ratio: f64,
    child: V,
}

impl<V: View> View for Wrap<V> {
    fn mount(self, cx: &mut Cx) {
        let tokens = parent_tokens(cx);
        let Some(node) = holder(cx, self.control, self.child) else {
            return;
        };
        let style = cx.inner.styles.resolve(
            &tokens,
            StyleQuery::new(ControlKind::AspectRatio, Slot::Root),
        );
        let data = cx.inner.data(node);
        data.decor.radius = Some(Corner::Points(style.radius.top_start));
        data.decor.clip = true;
        if self.ratio.is_finite() && self.ratio > 0.0 {
            data.aspect = Some(self.ratio);
            cx.inner.aspects.insert(node);
        }
        // The box takes the width it is offered, and its height follows.
        cx.inner.make_flexible(
            node,
            Flex {
                wide: true,
                tall: false,
            },
        );
    }
}

/// A titled frame around related controls. The group is named by
/// `title`, and `members` are its children in the tree. Each top-level
/// view of `members` stacks under the title. The group is as wide as its
/// widest member, and at least as wide as its title. `.disabled(true)` on
/// the group dims the frame and disables every member.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Binding, GroupBox, Switch, View};
///
/// fn shipping(express: Binding<bool>) -> impl View {
///     GroupBox::new("Shipping", Switch::new("Express", express))
/// }
/// ```
pub struct GroupBox<C> {
    title: Label,
    members: C,
}

impl<C: View> GroupBox<C> {
    /// A group box with `title` around `members`.
    pub fn new<M>(title: impl IntoLive<TextContent, M>, members: C) -> Self {
        Self {
            title: label(title),
            members,
        }
    }
}

impl<C: View> View for GroupBox<C> {
    fn mount(self, cx: &mut Cx) {
        let tokens = parent_tokens(cx);
        let layout = controls::GroupBox::layout(&cx.inner.styles, &tokens);
        let node = cx.node(LayoutStyle {
            kind: LayoutKind::Custom(SharedLayout::new(layout)),
            ..LayoutStyle::default()
        });
        // The group node carries the semantics; the card is decoration.
        let (title, env) = (self.title.clone(), cx.env.clone());
        cx.react(move |inner| {
            let access = &mut inner.data(node).access;
            access.role = Some(SemanticRole::Group);
            access.label = Some(text(&title, &env));
            inner.touch(node, Dirty::SEMANTICS);
        });
        let title = self.title;
        let card = Configured::custom(ControlKind::Card, move |env| {
            controls::GroupBox::hosted(text(&title, env))
        })
        .accessibility_hidden(true);
        let env = cx.env.clone();
        let mut inside = cx.within(node, env);
        inside.mount(card);
        inside.mount(self.members);
    }
}

/// A stack of pages that shows one page at a time: the view that `page`
/// makes for `current` fills the stack, as a child of the stack node.
/// There is a page for each of `titles`. The stack is a `Group` named
/// `name`, and its value is the title of the current page. No pages
/// report a build error, and a page past the end keeps the page that it
/// shows. Put a `Tabs` view with the same titles next to it to select the
/// page.
///
/// # Examples
///
/// ```
/// use choscorui_compose::view::{Cx, StackedContainer, Text, View};
///
/// struct Wizard;
///
/// impl View for Wizard {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let step = cx.state(0_usize);
///         let titles = vec!["Account".to_owned(), "Profile".to_owned()];
///         StackedContainer::new("Steps", titles, step, |page| {
///             Text::new(format!("Step {}", page + 1))
///         })
///     }
/// }
/// ```
pub struct StackedContainer<F> {
    name: Label,
    titles: Rc<dyn Items<String>>,
    current: Live<usize>,
    page: F,
}

impl<F, V> StackedContainer<F>
where
    F: Fn(usize) -> V + 'static,
    V: View,
{
    /// A stack named `name` with a page for each of `titles`, which shows
    /// page `current`, made by `page`.
    pub fn new<M, N>(
        name: impl IntoLive<TextContent, M>,
        titles: impl Items<String>,
        current: impl IntoLive<usize, N>,
        page: F,
    ) -> Self {
        Self {
            name: label(name),
            titles: Rc::new(titles),
            current: current.into_live(),
            page,
        }
    }
}

impl<F, V> View for StackedContainer<F>
where
    F: Fn(usize) -> V + 'static,
    V: View,
{
    fn mount(self, cx: &mut Cx) {
        let (name, titles) = (self.name, self.titles);
        let control = Configured::try_custom(ControlKind::Card, move |env| {
            let titles = titles.items(env);
            controls::StackedContainer::hosted(text(&name, env), titles.len(), 0)
                .map(|stack| stack.with_titles(titles))
        })
        .value(self.current.clone().map(to_position));
        // The current page fills the stack node.
        let (current, page) = (self.current, self.page);
        holder(cx, control, Dynamic::new(move || page(current.get())));
    }
}
