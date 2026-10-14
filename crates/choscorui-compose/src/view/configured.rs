//! A control view built from the environment, with chained options,
//! bindings, and actions. Each hosted control of `choscorui-controls` has
//! a view constructor that returns one. An application's own
//! `CustomControl` mounts through [`Configured::custom`].

use super::View;
use super::control::{ControlView, SharedNotes};
use super::control_ref::{ControlRef, Shared};
use super::env::Env;
use super::host::Cx;
use super::limits::{Limits, SharedLimits};
use super::state::{Binding, IntoLive, Live};
use crate::reactive::Runtime;
use choscorui_controls::{CommandId, ControlKind, ControlMessage, ControlValue, CustomControl};
use choscorui_core::Dirty;
use std::cell::RefCell;
use std::rc::Rc;

type Build<C> = Rc<dyn Fn(&Env) -> Result<C, String>>;
type Change<C> = Rc<dyn Fn(C, &Env) -> C>;
type Handler = Rc<dyn Fn(&ControlMessage)>;
type Update<C> = Rc<dyn Fn(&mut C, &Env)>;

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
    /// Changes of the mounted control in place.
    updates: Vec<Update<C>>,
    /// The reference that the app reads the mounted control through.
    reference: Option<ControlRef<C>>,
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
            updates: Vec::new(),
            reference: None,
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
        self.updates.push(Rc::new(change));
        self
    }

    /// Points `reference` at the mounted control, so the app can read it.
    #[must_use]
    pub fn control_ref(mut self, reference: &ControlRef<C>) -> Self {
        self.reference = Some(reference.clone());
        self
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
            updates,
            reference,
            value,
            open,
            handlers,
            limits,
            notes,
        } = self;
        let shared =
            (reference.is_some() || !updates.is_empty()).then(|| reference.unwrap_or_default());
        let updates = Rc::new(updates);
        let (built, applied) = (shared.clone(), updates.clone());
        let build: super::control::Build = Rc::new(move |env| {
            let control = build(env)?;
            let control = changes
                .iter()
                .fold(control, |control, change| change(control, env));
            let Some(reference) = &built else {
                return Ok(Box::new(control));
            };
            let control = Rc::new(RefCell::new(control));
            // The updates track their reads in reactions of their own.
            Runtime::untracked(|| {
                for update in applied.iter() {
                    update(&mut control.borrow_mut(), env);
                }
            });
            reference.set(control.clone());
            Ok(Box::new(Shared::new(control)))
        });
        let mut view = ControlView::configured(build, kind, value, open, handlers);
        view.limits = limits;
        view.notes = notes;
        let node = view.mount_node(cx);
        let Some(reference) = shared else {
            return;
        };
        for update in updates.iter().cloned() {
            let (reference, env) = (reference.clone(), cx.env().clone());
            cx.react(move |inner| {
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
    }
}
