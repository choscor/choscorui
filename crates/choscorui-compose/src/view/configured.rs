//! A control view built from the environment, with chained options,
//! bindings, and actions. The control constructors in this module return
//! one; any other control mounts through [`Configured::custom`].

use super::View;
use super::control::ControlView;
use super::env::Env;
use super::host::Cx;
use super::state::{Binding, IntoLive, Live};
use choscorui_controls::{CommandId, ControlKind, ControlMessage, ControlValue, CustomControl};
use std::rc::Rc;

type Build<C> = Rc<dyn Fn(&Env) -> Result<C, String>>;
type Change<C> = Rc<dyn Fn(C) -> C>;
type Handler = Rc<dyn Fn(&ControlMessage)>;

/// A control of type `C` as a view. The view builds the control from the
/// environment, so catalog strings follow the locale; options from
/// [`Configured::with`] apply after each build.
pub struct Configured<C> {
    kind: ControlKind,
    build: Build<C>,
    changes: Vec<Change<C>>,
    value: Option<Live<ControlValue>>,
    open: Option<Live<bool>>,
    handlers: Vec<Handler>,
}

impl<C: CustomControl + 'static> Configured<C> {
    /// A view of the control that `build` makes. `kind` is the control
    /// kind whose root slot local style modifiers change.
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
            value: None,
            open: None,
            handlers: Vec::new(),
        }
    }

    /// Changes the control after each build, for options such as
    /// `.with(|switch| switch.with_size(ControlSize::Sm))`.
    #[must_use]
    pub fn with(mut self, change: impl Fn(C) -> C + 'static) -> Self {
        self.changes.push(Rc::new(change));
        self
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

    /// Connects the control's value to `binding` in both directions.
    pub(crate) fn bind<T: Clone + PartialEq + Default + 'static>(
        self,
        binding: Binding<T>,
        to: fn(T) -> ControlValue,
        from: fn(&ControlValue) -> Option<T>,
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
    fn mount(self, cx: &mut Cx<'_>) {
        let Self {
            kind,
            build,
            changes,
            value,
            open,
            handlers,
        } = self;
        let build: super::control::Build = Rc::new(move |env| {
            let control = build(env)?;
            let control = changes
                .iter()
                .fold(control, |control, change| change(control));
            Ok(Box::new(control))
        });
        ControlView::configured(build, kind, value, open, handlers).mount(cx);
    }
}
