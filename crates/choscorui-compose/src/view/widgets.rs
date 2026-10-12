//! View constructors for controls. Each takes its content, a `Binding`
//! where the control edits a value, and an action closure where it fires
//! one. Behavior, keyboard handling, and semantics stay in the controls.

use super::View;
use super::control::ControlView;
use super::env::Env;
use super::host::Cx;
use super::state::{Binding, IntoLive, Live};
use super::text::TextContent;
use choscorui_controls as controls;
use choscorui_controls::{
    ButtonVariant, CommandId, ControlKind, ControlMessage, ControlSize, ControlValue,
};
use choscorui_core::Icon;
use std::rc::Rc;

/// The command id that view buttons report; views run closures instead.
const NO_COMMAND: CommandId = CommandId(0);

/// Connects a control's value to `binding` in both directions: the
/// control shows the binding's value, and a change by the user writes the
/// binding.
pub(crate) fn bind<T: Clone + PartialEq + Default + 'static>(
    view: ControlView,
    binding: Binding<T>,
    to: fn(T) -> ControlValue,
    from: fn(&ControlValue) -> Option<T>,
) -> ControlView {
    view.value(move || to(binding.get()))
        .on_message(move |message| {
            if let ControlMessage::Changed(value) = message
                && let Some(value) = from(value)
            {
                binding.set(value);
            }
        })
}

/// The control argument when there is one, otherwise the environment's.
fn pick<T: Clone + 'static>(own: &Option<Live<T>>, env: Option<T>) -> Option<T> {
    own.as_ref().map(Live::get).or(env)
}

/// A push button that runs `action`: `Button::new(t!("save"), move || …)`.
/// Without a variant or size of its own, it takes the environment's
/// `button_variant` and `control_size`.
pub struct Button {
    label: Live<TextContent>,
    action: Rc<dyn Fn()>,
    variant: Option<Live<ButtonVariant>>,
    size: Option<Live<ControlSize>>,
    leading: Option<Icon>,
    trailing: Option<Icon>,
}

impl Button {
    pub fn new<M>(label: impl IntoLive<TextContent, M>, action: impl Fn() + 'static) -> Self {
        Self {
            label: label.into_live(),
            action: Rc::new(action),
            variant: None,
            size: None,
            leading: None,
            trailing: None,
        }
    }

    /// The look of the button, instead of the environment's.
    #[must_use]
    pub fn variant<M>(mut self, variant: impl IntoLive<ButtonVariant, M>) -> Self {
        self.variant = Some(variant.into_live());
        self
    }

    /// The size of the button, instead of the environment's.
    #[must_use]
    pub fn size<M>(mut self, size: impl IntoLive<ControlSize, M>) -> Self {
        self.size = Some(size.into_live());
        self
    }

    /// An icon before the label, or the only content of an icon size.
    #[must_use]
    pub fn icon(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// An icon after the label.
    #[must_use]
    pub fn trailing_icon(mut self, icon: Icon) -> Self {
        self.trailing = Some(icon);
        self
    }
}

impl View for Button {
    fn mount(self, cx: &mut Cx<'_>) {
        let Self {
            label,
            action,
            variant,
            size,
            leading,
            trailing,
        } = self;
        ControlView::build(move |env: &Env| {
            let variant = pick(&variant, env.button_variant()).unwrap_or_default();
            let size = pick(&size, env.control_size()).unwrap_or_default();
            let mut button = controls::Button::new(label.get().resolve(env), NO_COMMAND)
                .with_variant(variant)
                .with_size(size);
            if let Some(icon) = leading {
                button = button.with_icon(icon);
            }
            if let Some(icon) = trailing {
                button = button.with_trailing_icon(icon);
            }
            Box::new(button)
        })
        .kind(ControlKind::Button)
        .on_message(move |message| {
            if *message == ControlMessage::Activated {
                action();
            }
        })
        .mount(cx);
    }
}

/// A checkbox that edits `checked`.
pub struct Checkbox {
    label: Live<TextContent>,
    checked: Binding<bool>,
}

impl Checkbox {
    pub fn new<M>(label: impl IntoLive<TextContent, M>, checked: Binding<bool>) -> Self {
        Self {
            label: label.into_live(),
            checked,
        }
    }
}

impl View for Checkbox {
    fn mount(self, cx: &mut Cx<'_>) {
        let Self { label, checked } = self;
        let view = ControlView::build(move |env: &Env| {
            Box::new(controls::Checkbox::new(label.get().resolve(env), false))
        })
        .kind(ControlKind::Checkbox);
        bind(view, checked, ControlValue::Bool, |value| match value {
            ControlValue::Bool(checked) => Some(*checked),
            _ => None,
        })
        .mount(cx);
    }
}
