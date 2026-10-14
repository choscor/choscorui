//! A control view built from the environment, with chained options,
//! bindings, and actions. Each hosted control of `choscorui-controls` has
//! a view constructor that returns one. An application's own
//! `CustomControl` mounts through [`Configured::custom`].

use super::View;
use super::control::{ControlView, SharedNotes};
use super::control_ref::{ControlRef, Shared};
use super::env::Env;
use super::host::{Cx, Inner};
use super::limits::{Limits, SharedLimits};
use super::state::{Binding, IntoLive, Live};
use crate::reactive::Runtime;
use choscorui_controls::{CommandId, ControlKind, ControlMessage, ControlValue, CustomControl};
use choscorui_core::{Dirty, NodeId};
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

type Build<C> = Rc<dyn Fn(&Env) -> Result<C, String>>;
type Change<C> = Rc<dyn Fn(C, &Env) -> C>;
type Handler = Rc<dyn Fn(&ControlMessage)>;
type Update<C> = Rc<dyn Fn(&mut C, &Env)>;
/// Makes the mounted control of a build.
type Wrap<C> = Rc<dyn Fn(C, &Env) -> Box<dyn CustomControl>>;
/// Runs with the node of the control after the mount.
type Mounted = Box<dyn FnOnce(&mut Cx, NodeId)>;

/// The control that the updates and the references of a view share.
struct Sharing<C> {
    /// The reference of the updates, followed by the app's references.
    references: RefCell<Vec<ControlRef<C>>>,
    updates: RefCell<Vec<Update<C>>>,
}

impl<C: CustomControl + 'static> Sharing<C> {
    /// Shares the control of each build: runs the updates on it, and
    /// points the references at it.
    fn wrap(self: &Rc<Self>) -> Wrap<C> {
        let sharing = self.clone();
        Rc::new(move |control, env| {
            let control = Rc::new(RefCell::new(control));
            // The updates track their reads in reactions of their own.
            Runtime::untracked(|| {
                for update in sharing.updates.borrow().iter() {
                    update(&mut control.borrow_mut(), env);
                }
            });
            for reference in sharing.references.borrow().iter() {
                reference.set(control.clone());
            }
            Box::new(Shared::new(control))
        })
    }

    /// The reference of the updates.
    fn own(&self) -> ControlRef<C> {
        self.references.borrow()[0].clone()
    }
}

/// Runs `update` on the control of `reference` in a reaction, and lays
/// out the node again.
fn react_update<C: 'static>(
    cx: &mut Cx,
    node: NodeId,
    reference: ControlRef<C>,
    update: Update<C>,
) {
    let env = cx.env().clone();
    cx.react(move |inner: &mut Inner| {
        let Some(control) = reference.get() else {
            return;
        };
        let Ok(mut control) = control.try_borrow_mut() else {
            return;
        };
        update(&mut control, &env);
        inner.touch(node, Dirty::LAYOUT.with(Dirty::SEMANTICS));
    });
}

/// A control of type `C` as a view. The view builds the control from the
/// environment, so catalog strings follow the locale; options from
/// [`Configured::with`] and [`Configured::with_env`] apply after each
/// build.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::{ControlContext, ControlEvent, ControlKind, CustomControl};
/// # use choscorui_controls::{MeasureContext, PaintContext};
/// # use choscorui_core::{Color, EventReply, Measured, SemanticNode, SemanticRole, Size};
/// # struct Swatch(Color);
/// # impl CustomControl for Swatch {
/// #     fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
/// #         Measured::new(cx.constraints.constrain(Size::new(24.0, 24.0)))
/// #     }
/// #     fn event(&mut self, _: &mut ControlContext<'_>, _: &ControlEvent) -> EventReply {
/// #         EventReply::Continue
/// #     }
/// #     fn paint(&self, cx: &mut PaintContext<'_>) {
/// #         cx.list.fill_rect(cx.bounds, self.0);
/// #     }
/// #     fn semantics(&self) -> SemanticNode {
/// #         SemanticNode {
/// #             role: SemanticRole::Image,
/// #             name: "Swatch".into(),
/// #             description: String::new(),
/// #             value: None,
/// #             state: Default::default(),
/// #             relations: Vec::new(),
/// #             actions: Vec::new(),
/// #             text: None,
/// #         }
/// #     }
/// # }
/// // `Swatch` is the `CustomControl` of the `ControlView` example.
/// struct Theme;
///
/// impl View for Theme {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let dark = cx.state(false);
///         Configured::custom(ControlKind::Card, move |_env| {
///             Swatch(if dark.get() { Color::BLACK } else { Color::WHITE })
///         })
///     }
/// }
/// ```
pub struct Configured<C> {
    kind: ControlKind,
    build: Build<C>,
    changes: Vec<Change<C>>,
    /// The shared control of the updates and the references, a
    /// `Sharing<C>`. It is type-erased, so that a view without updates
    /// or references compiles no code for it.
    sharing: Option<Rc<dyn Any>>,
    /// Makes the mounted control from a build, when it is shared.
    wrap: Option<Wrap<C>>,
    /// What runs with the node after the mount.
    mounted: Vec<Mounted>,
    value: Option<Live<ControlValue>>,
    open: Option<Live<bool>>,
    handlers: Vec<Handler>,
    /// The bounds of a range view, which clamp the value.
    limits: Option<SharedLimits>,
    /// Invalid options that the view clamped or skipped.
    notes: SharedNotes,
}

impl<C: CustomControl + 'static> Configured<C> {
    /// A view of the control that `build` makes, for an application's own
    /// `CustomControl`. `kind` is the control kind whose root slot local
    /// style modifiers change.
    pub fn custom(kind: ControlKind, build: impl Fn(&Env) -> C + 'static) -> Self {
        Self::try_custom(kind, move |env| {
            Ok::<_, std::convert::Infallible>(build(env))
        })
    }

    /// A view of the control that `build` makes, for a constructor that
    /// can fail. A failed build reports [`ViewError::Control`] and keeps
    /// the control that the node has.
    ///
    /// [`ViewError::Control`]: super::ViewError::Control
    pub fn try_custom<E: std::fmt::Display>(
        kind: ControlKind,
        build: impl Fn(&Env) -> Result<C, E> + 'static,
    ) -> Self {
        Self {
            kind,
            build: Rc::new(move |env| build(env).map_err(|error| error.to_string())),
            changes: Vec::new(),
            sharing: None,
            wrap: None,
            mounted: Vec::new(),
            value: None,
            open: None,
            handlers: Vec::new(),
            limits: None,
            notes: SharedNotes::default(),
        }
    }

    /// Changes the control after each build, for options such as
    /// `.with(|switch| switch.with_size(ControlSize::Sm))`.
    #[must_use]
    pub fn with(self, change: impl Fn(C) -> C + 'static) -> Self {
        self.with_env(move |control, _| change(control))
    }

    /// Changes the control after each build with the environment, for
    /// options whose text comes from the catalog, such as
    /// `.with_env(|select, env| select.with_placeholder(env.text("pick", &[])))`.
    /// A state that `change` reads builds the control again when it
    /// changes.
    #[must_use]
    pub fn with_env(mut self, change: impl Fn(C, &Env) -> C + 'static) -> Self {
        self.changes.push(Rc::new(change));
        self
    }

    /// Changes the mounted control in place with `change`, after each
    /// build and whenever a state that `change` reads changes. Unlike
    /// [`Configured::with_env`], a change here does not build the control
    /// again, so the control keeps what the user did with it, such as the
    /// scroll offset and the selection of a list.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::ViewLayout;
    /// # use choscorui_core::{ItemModel, StringListModel};
    /// struct Photos;
    ///
    /// impl View for Photos {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let (selection, grid) = (cx.state(Vec::new()), cx.state(false));
    ///         let model = || Box::new(StringListModel::from_strings(["Hồ Gươm"])) as Box<dyn ItemModel>;
    ///         List::new("Photos", model, selection.binding()).update(move |view, _| {
    ///             view.set_layout(if grid.get() {
    ///                 ViewLayout::Grid { item_width: 120.0 }
    ///             } else {
    ///                 ViewLayout::List
    ///             });
    ///         })
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn update(mut self, change: impl Fn(&mut C, &Env) + 'static) -> Self {
        let sharing = self.sharing();
        let update: Update<C> = Rc::new(change);
        sharing.updates.borrow_mut().push(update.clone());
        let reference = sharing.own();
        self.mounted.push(Box::new(move |cx, node| {
            react_update(cx, node, reference, update);
        }));
        self
    }

    /// Points `reference` at the mounted control, so the app can read it.
    #[must_use]
    pub fn control_ref(mut self, reference: &ControlRef<C>) -> Self {
        let sharing = self.sharing();
        sharing.references.borrow_mut().push(reference.clone());
        self
    }

    /// The shared control of the view, which it makes at the first call.
    fn sharing(&mut self) -> Rc<Sharing<C>> {
        if let Some(sharing) = self.sharing.clone().and_then(|any| any.downcast().ok()) {
            return sharing;
        }
        let sharing = Rc::new(Sharing {
            references: RefCell::new(vec![ControlRef::new()]),
            updates: RefCell::default(),
        });
        self.wrap = Some(sharing.wrap());
        self.sharing = Some(sharing.clone());
        sharing
    }

    /// Gives the view `limits`, which its build reads and which clamp its
    /// value.
    pub(super) fn limited(mut self, limits: SharedLimits) -> Self {
        self.limits = Some(limits);
        self
    }

    /// Changes the limits of a range view. `change` returns a note when
    /// it clamped an invalid option.
    pub(super) fn change_limits(self, change: impl FnOnce(&mut Limits) -> Option<String>) -> Self {
        if let Some(limits) = &self.limits {
            let mut next = limits.get();
            if let Some(note) = change(&mut next) {
                self.notes.borrow_mut().push(note);
            }
            limits.set(next);
        }
        self
    }

    /// Changes the control after each build. `check` returns a note when
    /// it skipped an invalid option; the view reports the note once as
    /// [`ViewError::Clamped`] and keeps the rest of the control.
    ///
    /// [`ViewError::Clamped`]: super::ViewError::Clamped
    pub(super) fn with_check(self, check: impl Fn(&mut C) -> Option<String> + 'static) -> Self {
        let notes = self.notes.clone();
        self.with(move |mut control| {
            if let Some(note) = check(&mut control) {
                notes.borrow_mut().push(note);
            }
            control
        })
    }

    /// Gives the control this value now and whenever it changes.
    #[must_use]
    pub fn value<M>(mut self, value: impl IntoLive<ControlValue, M>) -> Self {
        self.value = Some(value.into_live());
        self
    }

    /// Opens or closes the control with `open`, and writes the user's
    /// opening and closing back to it.
    #[must_use]
    pub fn open(mut self, open: Binding<bool>) -> Self {
        self.open = Some(Live::Computed(Rc::new(move || open.get())));
        self.on_message(move |message| {
            if let ControlMessage::OpenChanged(value) = message {
                open.set(*value);
            }
        })
    }

    /// Calls `handler` with each message of the control.
    #[must_use]
    pub fn on_message(mut self, handler: impl Fn(&ControlMessage) + 'static) -> Self {
        self.handlers.push(Rc::new(handler));
        self
    }

    /// Runs `action` when the control runs its action.
    #[must_use]
    pub fn on_activate(self, action: impl Fn() + 'static) -> Self {
        self.on_message(move |message| {
            if *message == ControlMessage::Activated {
                action();
            }
        })
    }

    /// Runs `action` with the index of an item that the user activates,
    /// for example a menu item or a breadcrumb link.
    #[must_use]
    pub fn on_item(self, action: impl Fn(usize) + 'static) -> Self {
        self.on_message(move |message| {
            if let ControlMessage::ItemActivated(index) = message {
                action(*index);
            }
        })
    }

    /// Runs `action` with the command of a menu action that the user
    /// chooses.
    #[must_use]
    pub fn on_command(self, action: impl Fn(CommandId) + 'static) -> Self {
        self.on_message(move |message| {
            if let ControlMessage::Command(command) = message {
                action(*command);
            }
        })
    }

    /// Runs `action` when the user dismisses the control.
    #[must_use]
    pub fn on_dismiss(self, action: impl Fn() + 'static) -> Self {
        self.on_message(move |message| {
            if *message == ControlMessage::Dismissed {
                action();
            }
        })
    }

    /// Connects the control's value to `binding` in both directions. `to`
    /// turns the bound value into the value that the control takes in
    /// [`CustomControl::set_value`]. `from` reads a
    /// [`ControlMessage::Changed`] value, and returns `None` for a value
    /// of another kind, which leaves the binding as it is. Use it to bind
    /// a control that has no view constructor of its own:
    ///
    /// ```
    /// use choscorui_compose::view::{Binding, Configured};
    /// use choscorui_controls::{ControlKind, ControlValue, Toggle};
    ///
    /// fn starred(on: Binding<bool>) -> Configured<Toggle> {
    ///     Configured::custom(ControlKind::Toggle, |_| Toggle::new("Star", false)).bind(
    ///         on,
    ///         ControlValue::Bool,
    ///         |value| match value {
    ///             ControlValue::Bool(on) => Some(*on),
    ///             _ => None,
    ///         },
    ///     )
    /// }
    /// ```
    #[must_use]
    pub fn bind<T: Clone + PartialEq + Default + 'static>(
        self,
        binding: Binding<T>,
        to: impl Fn(T) -> ControlValue + 'static,
        from: impl Fn(&ControlValue) -> Option<T> + 'static,
    ) -> Self {
        self.value(move || to(binding.get()))
            .on_message(move |message| {
                if let ControlMessage::Changed(value) = message
                    && let Some(value) = from(value)
                {
                    binding.set(value);
                }
            })
    }
}

impl<C: CustomControl + 'static> View for Configured<C> {
    fn mount(self, cx: &mut Cx) {
        let Self {
            kind,
            build,
            changes,
            sharing: _,
            wrap,
            mounted,
            value,
            open,
            handlers,
            limits,
            notes,
        } = self;
        let build: super::control::Build = Rc::new(move |env| {
            let control = build(env)?;
            let control = changes
                .iter()
                .fold(control, |control, change| change(control, env));
            Ok(match &wrap {
                Some(wrap) => wrap(control, env),
                None => Box::new(control),
            })
        });
        let mut view = ControlView::configured(build, kind, value, open, handlers);
        view.limits = limits;
        view.notes = notes;
        let node = view.mount_node(cx);
        run_mounted(cx, node, mounted);
    }
}

/// Runs what a view does with its node after the mount.
fn run_mounted(cx: &mut Cx, node: NodeId, mounted: Vec<Mounted>) {
    for mounted in mounted {
        mounted(cx, node);
    }
}
