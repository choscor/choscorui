//! Controls as views: [`ControlView`] mounts any control on a node and
//! connects its value, open state, messages, and the environment's
//! disabled state.

use super::View;
use super::env::Env;
use super::host::{Cx, Handler, Inner, ViewError};
use super::state::{IntoLive, Live};
use choscorui_controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue, CustomControl,
    MeasureContext, PaintContext,
};
use choscorui_core::{
    Dirty, EventReply, Measured, NodeId, SemanticNode, SemanticRole, SemanticState, Size,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The low-level extension point for new controls: a type that measures,
/// paints, reports semantics, and handles events through the control
/// protocol. Mount one with [`ControlView::new`].
pub trait CustomView: CustomControl + 'static {}

impl<T: CustomControl + 'static> CustomView for T {}

/// Builds a control from the environment, or says why it cannot.
pub(crate) type Build = Rc<dyn Fn(&Env) -> Result<Box<dyn CustomControl>, String>>;

/// Where a [`ControlView`] gets its control.
enum Source {
    Fixed(Box<dyn CustomControl>),
    Built(Build),
}

/// A control mounted as a view. The control keeps its behavior, keyboard
/// handling, and semantics; the view configures it and connects it to
/// state.
pub struct ControlView {
    source: Source,
    kind: Option<ControlKind>,
    value: Option<Live<ControlValue>>,
    open: Option<Live<bool>>,
    handlers: Vec<Handler>,
}

impl ControlView {
    pub fn new(control: impl CustomView) -> Self {
        Self::boxed(Box::new(control))
    }

    pub fn boxed(control: Box<dyn CustomControl>) -> Self {
        Self::with_source(Source::Fixed(control))
    }

    /// A control that `build` makes from the environment. When state or
    /// an environment value that `build` read changes, the view builds
    /// the control again on the same node. The node keeps its focus, and
    /// the view gives the new control the current value and open state.
    pub fn build(build: impl Fn(&Env) -> Box<dyn CustomControl> + 'static) -> Self {
        Self::try_build(move |env| Ok::<_, std::convert::Infallible>(build(env)))
    }

    /// Like [`ControlView::build`], for a control whose constructor can
    /// fail. A failed build reports [`ViewError::Control`] and keeps the
    /// control that the node has; before the first good build, the node
    /// has an empty placeholder.
    pub fn try_build<E: std::fmt::Display>(
        build: impl Fn(&Env) -> Result<Box<dyn CustomControl>, E> + 'static,
    ) -> Self {
        Self::with_source(Source::Built(Rc::new(move |env| {
            build(env).map_err(|error| error.to_string())
        })))
    }

    /// A view of the control that `build` makes, with all its options.
    pub(crate) fn configured(
        build: Build,
        kind: ControlKind,
        value: Option<Live<ControlValue>>,
        open: Option<Live<bool>>,
        handlers: Vec<Handler>,
    ) -> Self {
        Self {
            source: Source::Built(build),
            kind: Some(kind),
            value,
            open,
            handlers,
        }
    }

    fn with_source(source: Source) -> Self {
        Self {
            source,
            kind: None,
            value: None,
            open: None,
            handlers: Vec::new(),
        }
    }

    /// The control kind whose root slot local style modifiers change.
    #[must_use]
    pub fn kind(mut self, kind: ControlKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Gives the control this value now and whenever it changes, as
    /// [`CustomControl::set_value`] describes.
    #[must_use]
    pub fn value<M>(mut self, value: impl IntoLive<ControlValue, M>) -> Self {
        self.value = Some(value.into_live());
        self
    }

    /// Opens or closes the control's popup with this value.
    #[must_use]
    pub fn open<M>(mut self, open: impl IntoLive<bool, M>) -> Self {
        self.open = Some(open.into_live());
        self
    }

    /// Calls `handler` with each message that the control emits.
    #[must_use]
    pub fn on_message(mut self, handler: impl Fn(&ControlMessage) + 'static) -> Self {
        self.handlers.push(Rc::new(handler));
        self
    }
}

/// The value and open state that the control last had, so a rebuilt
/// control gets them and an unchanged value is not set again.
#[derive(Default)]
struct Current {
    value: RefCell<Option<ControlValue>>,
    open: Cell<Option<bool>>,
}

impl Current {
    /// Gives `control` the current value and open state.
    fn restore(&self, control: &mut dyn CustomControl, time: f64) {
        if let Some(value) = &*self.value.borrow() {
            control.set_value(value);
        }
        if let Some(open) = self.open.get() {
            control.set_open(open, time);
        }
    }
}

impl View for ControlView {
    // Out of line, so that each `Configured<C>` adds only its build.
    #[inline(never)]
    fn mount(self, cx: &mut Cx) {
        let current: Rc<Current> = Rc::default();
        let node = match self.source {
            Source::Fixed(control) => cx.control(control),
            Source::Built(build) => mount_built(cx, build, current.clone()),
        };
        {
            let recorder = current.clone();
            let data = cx.inner.data(node);
            // The value that the user chose is current before the
            // handlers write it to state.
            data.handlers
                .push(Rc::new(move |message: &ControlMessage| match message {
                    ControlMessage::Changed(value) => {
                        *recorder.value.borrow_mut() = Some(value.clone())
                    }
                    ControlMessage::OpenChanged(open) => recorder.open.set(Some(*open)),
                    _ => {}
                }));
            data.handlers.extend(self.handlers);
            data.kind = self.kind;
        }
        let disabled = cx.env.disabled.clone();
        cx.bind(disabled, move |inner, disabled| {
            set_enabled(inner, node, !disabled)
        });
        if let Some(value) = self.value {
            let current = current.clone();
            cx.bind(value, move |inner, value| {
                if current.value.borrow().as_ref() == Some(&value) {
                    return;
                }
                inner.controls.set_value(&mut inner.tree, node, &value);
                *current.value.borrow_mut() = Some(value);
                inner.touch(node, Dirty::LAYOUT.with(Dirty::SEMANTICS));
            });
        }
        if let Some(open) = self.open {
            cx.bind(open, move |inner, open| {
                if current.open.get() == Some(open) {
                    return;
                }
                inner.controls.set_open(&mut inner.tree, node, open);
                current.open.set(Some(open));
                inner.touch(node, Dirty::PAINT.with(Dirty::SEMANTICS));
            });
        }
    }
}

/// Mounts the control that `build` makes, and builds it again on the same
/// node when what `build` read changes.
fn mount_built(cx: &mut Cx, build: Build, current: Rc<Current>) -> NodeId {
    let env = cx.env.clone();
    let first: Rc<RefCell<Option<Box<dyn CustomControl>>>> = Rc::default();
    let mounted: Rc<Cell<Option<NodeId>>> = Rc::default();
    {
        let (first, mounted) = (first.clone(), mounted.clone());
        cx.react(move |inner| {
            let mut control = match build(&env) {
                Ok(control) => control,
                Err(error) => {
                    inner.report(ViewError::Control(error));
                    if mounted.get().is_some() {
                        return;
                    }
                    Box::new(Placeholder)
                }
            };
            match mounted.get() {
                None => *first.borrow_mut() = Some(control),
                Some(node) => {
                    current.restore(control.as_mut(), inner.time);
                    if let Err(error) = inner.controls.replace(&mut inner.tree, node, control) {
                        inner.report(ViewError::Tree(error));
                    }
                    inner.touch(node, Dirty::LAYOUT.with(Dirty::SEMANTICS));
                }
            }
        });
    }
    let control = first.borrow_mut().take();
    let node = match control {
        Some(control) => cx.control(control),
        None => cx.parent,
    };
    mounted.set(Some(node));
    node
}

/// What a node shows until its control builds.
struct Placeholder;

impl CustomControl for Placeholder {
    fn measure(&mut self, _cx: &MeasureContext<'_>) -> Measured {
        Measured::new(Size::ZERO)
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, _cx: &mut PaintContext<'_>) {}

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Group,
            name: String::new(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

fn set_enabled(inner: &mut Inner, node: NodeId, enabled: bool) {
    if inner.controls.is_enabled(node) == enabled {
        return;
    }
    if let Err(error) = inner.controls.set_enabled(&mut inner.tree, node, enabled) {
        inner.report(ViewError::Tree(error));
    }
    inner.touch(node, Dirty::PAINT.with(Dirty::SEMANTICS));
}
