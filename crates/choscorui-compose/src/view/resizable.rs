//! Resizable panel groups as views. The node holds the controls' hosted
//! panel group. With two panes, the group node paints the handle and
//! takes the drag, the keys, and the actions. With three or more, each
//! handle is a child node of the group, before the panes, that forwards
//! to the same group, so each handle is its own focus stop and slider.
//! The node lowers onto a core custom layout that puts each pane view
//! and each handle node in its rectangle of that same control, so the
//! panes follow each handle at the next layout.

use super::View;
use super::control::ControlView;
use super::host::{Cx, Flex, Inner, ViewError};
use super::layout::parent_tokens;
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Label, label, text};
use choscorui_controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue, CustomControl,
    HostedResizable, MeasureContext, PaintContext, Resizable as Group, Slot, SplitOrientation,
    StyleQuery,
};
use choscorui_core::{
    ChildMeasure, Constraints, CustomLayout, Dirty, EventReply, LayoutDirection, LayoutKind,
    LayoutStyle, Localizer, Measured, NodeId, Point, Rect, SemanticAction, SemanticNode,
    SharedLayout, Size,
};
use std::cell::RefCell;
use std::rc::Rc;

/// The panel group that the control on the node and the pane layout
/// share.
type Model = Rc<RefCell<HostedResizable>>;

/// Panes with a handle between each two neighbors. Each top-level view of
/// `panes` is one pane, in order. `sizes` holds the size of each pane but
/// the last, which takes the rest, along the split. A drag or a key moves
/// one handle, changes only the two panes beside it, and writes `sizes`.
/// The keys, the actions, the semantics, and the look are those of the
/// hosted [`Resizable`](choscorui_controls::Resizable) panel group. With
/// three or more panes, each handle is its own node and focus stop, with
/// its own slider semantics, in a group node. A horizontal group mirrors
/// in right-to-left layouts. The group takes the space it is offered.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::SplitOrientation;
/// struct Split;
///
/// impl View for Split {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let sizes = cx.state(vec![240.0]);
///         let panes = (Text::new("Sidebar"), Text::new("Content"));
///         Resizable::new("Panes", SplitOrientation::Horizontal, panes, sizes.binding())
///     }
/// }
/// ```
pub struct Resizable<P> {
    name: Label,
    orientation: SplitOrientation,
    panes: P,
    sizes: Binding<Vec<f64>>,
    minimums: Option<Vec<f64>>,
    grip: bool,
}

impl<P: View> Resizable<P> {
    /// A group named `name` for assistive technology, which splits along
    /// `orientation`. Each pane has no minimum size.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        orientation: SplitOrientation,
        panes: P,
        sizes: Binding<Vec<f64>>,
    ) -> Self {
        Self {
            name: label(name),
            orientation,
            panes,
            sizes,
            minimums: None,
            grip: false,
        }
    }

    /// The smallest size of each pane, one for each pane.
    #[must_use]
    pub fn minimums(mut self, minimums: Vec<f64>) -> Self {
        self.minimums = Some(minimums);
        self
    }

    /// Shows a grip on each handle.
    #[must_use]
    pub fn grip(mut self, grip: bool) -> Self {
        self.grip = grip;
        self
    }
}

impl<P: View> View for Resizable<P> {
    fn mount(self, cx: &mut Cx) {
        let Some((node, model)) = group_node(
            cx,
            self.name,
            self.orientation,
            self.sizes,
            self.minimums,
            self.grip,
        ) else {
            return;
        };
        let env = cx.env.clone();
        let mut inside = cx.within(node, env);
        let handles = handle_nodes(&mut inside, &model, self.sizes);
        inside.mount(self.panes);
        follow(cx, node, handles, model, self.sizes);
    }
}

/// Mounts one node for each handle of a group with handle nodes, as the
/// first children of the group node, and returns them. Each node writes
/// `sizes` when its handle moves, and refreshes the semantics of every
/// handle, as a move changes the range of its neighbors.
fn handle_nodes(cx: &mut Cx, model: &Model, sizes: Binding<Vec<f64>>) -> Rc<[NodeId]> {
    let count = {
        let group = model.borrow();
        if group.handle_count() < 2 {
            return Rc::from([]);
        }
        group.handle_count()
    };
    let handles: Rc<[NodeId]> = (0..count)
        .map(|index| {
            cx.control(Box::new(HandleControl {
                model: model.clone(),
                index,
            }))
        })
        .collect();
    for &handle in handles.iter() {
        let (model, all) = (model.clone(), handles.clone());
        cx.inner.data(handle).internal = Some(Rc::new(
            move |inner: &mut Inner, message: &ControlMessage| {
                if let ControlMessage::Changed(_) = message {
                    write(&model, sizes);
                    for &node in all.iter() {
                        inner.touch(node, Dirty::SEMANTICS);
                    }
                }
            },
        ));
    }
    handles
}

/// The handle extent along the split, from the `Resizable` recipe: the
/// hit area around the line, and the grip.
fn handle_extent(cx: &Cx, grip: bool) -> f64 {
    let tokens = parent_tokens(cx);
    let style = |slot| {
        cx.inner
            .styles
            .resolve(&tokens, StyleQuery::new(ControlKind::Resizable, slot))
    };
    let line = style(Slot::Handle);
    let extent = line.padding.horizontal().max(line.min_width);
    if grip {
        extent.max(style(Slot::Thumb).min_width)
    } else {
        extent
    }
}

/// Mounts the control node of the group with its pane layout, and returns
/// the node and the shared group. A group that cannot be built reports
/// [`ViewError::Control`] and mounts nothing.
#[inline(never)]
fn group_node(
    cx: &mut Cx,
    name: Label,
    orientation: SplitOrientation,
    sizes: Binding<Vec<f64>>,
    minimums: Option<Vec<f64>>,
    grip: bool,
) -> Option<(NodeId, Model)> {
    let initial = sizes.get();
    let minimums = minimums.unwrap_or_else(|| vec![0.0; initial.len() + 1]);
    let handle = handle_extent(cx, grip);
    // Two panes keep the single slider; more get a node for each handle.
    let handle_nodes = minimums.len() > 2;
    let build = {
        let minimums = minimums.clone();
        move |label: String, sizes: Vec<f64>| {
            Group::hosted_group(label, orientation, minimums.clone(), handle, sizes)
                .map(|group| group.with_grip(grip).with_handle_nodes(handle_nodes))
        }
    };
    let model: Model = match build(text(&name, &cx.env), initial) {
        Ok(group) => Rc::new(RefCell::new(group)),
        Err(error) => {
            cx.inner.report(ViewError::Control(error.to_string()));
            return None;
        }
    };
    let shared = model.clone();
    // A new build, for example after a locale change, keeps the sizes.
    let control = ControlView::try_build(move |env| {
        let group = build(text(&name, env), stored(&shared.borrow()))?;
        *shared.borrow_mut() = group;
        Ok::<_, choscorui_controls::ContainerError>(
            Box::new(PaneControl(shared.clone())) as Box<dyn CustomControl>
        )
    })
    .kind(ControlKind::Resizable);
    let read = model.clone();
    let control = control.on_message(move |message| {
        if let ControlMessage::Changed(_) = message {
            write(&read, sizes);
        }
    });
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
    let style = LayoutStyle {
        kind: LayoutKind::Custom(SharedLayout::new(PaneLayout {
            model: model.clone(),
            orientation,
        })),
        ..LayoutStyle::default()
    };
    cx.inner.set_style(node, style);
    cx.inner.make_flexible(node, Flex::BOTH);
    Some((node, model))
}

/// Writes the sizes of the group to `sizes` when they differ.
fn write(model: &Model, sizes: Binding<Vec<f64>>) {
    let next = stored(&model.borrow());
    if sizes.try_get().ok().as_ref() != Some(&next) {
        sizes.set(next);
    }
}

/// Gives the group the sizes of `sizes` whenever they change, and
/// refreshes the semantics of the group and of each handle node.
fn follow(
    cx: &mut Cx,
    node: NodeId,
    handles: Rc<[NodeId]>,
    model: Model,
    sizes: Binding<Vec<f64>>,
) {
    cx.react(move |inner| {
        let next = sizes.get();
        if next == stored(&model.borrow()) {
            return;
        }
        if model.borrow_mut().set_sizes(&next) {
            inner.touch(node, Dirty::LAYOUT.with(Dirty::SEMANTICS));
            for &handle in handles.iter() {
                inner.touch(handle, Dirty::SEMANTICS);
            }
        } else {
            inner.report(ViewError::Control(format!(
                "the pane sizes {next:?} do not fit the panel group"
            )));
        }
    });
}

/// The size of each pane but the last, as the group lays them out.
fn stored(group: &HostedResizable) -> Vec<f64> {
    let mut sizes = group.sizes();
    sizes.pop();
    sizes
}

/// Places each pane view in its pane rectangle of the group, and each
/// handle node, which come first, in its handle rectangle.
struct PaneLayout {
    model: Model,
    orientation: SplitOrientation,
}

impl CustomLayout for PaneLayout {
    fn layout(
        &self,
        space: Constraints,
        direction: LayoutDirection,
        children: &mut dyn ChildMeasure,
    ) -> (Size, Vec<Rect>) {
        let Ok(group) = self.model.try_borrow() else {
            return (space.min, Vec::new());
        };
        let first_pane = if group.handle_count() >= 2 {
            group.handle_count()
        } else {
            0
        };
        let horizontal = self.orientation == SplitOrientation::Horizontal;
        let (along_max, cross_max, along_min, cross_min) = if horizontal {
            let (max, min) = (space.max, space.min);
            (max.width, max.height, min.width, min.height)
        } else {
            let (max, min) = (space.max, space.min);
            (max.height, max.width, min.height, min.width)
        };
        let along = if along_max.is_finite() {
            along_max
        } else {
            group.minimum_extent().max(along_min)
        };
        let cross = if cross_max.is_finite() {
            cross_max
        } else {
            // The panes are as tall (or as wide) as the largest pane.
            let natural = Constraints::UNBOUNDED;
            (first_pane..children.len())
                .map(|index| {
                    let size = children.measure(index, natural).size;
                    if horizontal { size.height } else { size.width }
                })
                .fold(cross_min, f64::max)
        };
        let size = if horizontal {
            Size::new(along, cross)
        } else {
            Size::new(cross, along)
        };
        let bounds = Rect::new(Point::new(0.0, 0.0), size);
        let (panes, handles) = group.rects_in(bounds, direction == LayoutDirection::Rtl);
        if first_pane == 0 {
            return (size, panes);
        }
        (size, handles.into_iter().chain(panes).collect())
    }
}

/// The control on a handle node: one handle of the shared hosted panel
/// group.
struct HandleControl {
    model: Model,
    index: usize,
}

impl CustomControl for HandleControl {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.model.borrow_mut().handle_event(self.index, cx, event)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.model.borrow().paint_handle(self.index, cx);
    }

    fn semantics(&self) -> SemanticNode {
        self.model.borrow().handle_semantics(self.index)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        self.model
            .borrow_mut()
            .handle_perform(self.index, cx, action)
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.model.borrow_mut().set_enabled(enabled);
    }
}

/// The control on the group's node: the shared hosted panel group.
struct PaneControl(Model);

impl CustomControl for PaneControl {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.0.borrow_mut().measure(cx)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.0.borrow_mut().arrange(cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.0.borrow_mut().event(cx, event)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.0.borrow().paint(cx);
    }

    fn semantics(&self) -> SemanticNode {
        self.0.borrow().semantics()
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        self.0.borrow().semantics_in(strings)
    }

    fn focusable(&self) -> bool {
        self.0.borrow().focusable()
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.0.borrow_mut().perform(cx, action, value)
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.0.borrow_mut().set_enabled(enabled);
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        self.0.borrow_mut().set_value(value)
    }
}
