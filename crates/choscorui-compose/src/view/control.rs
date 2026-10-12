//! Controls as views: [`ControlView`] mounts any control on a node and
//! connects its value, open state, messages, and the environment's
//! disabled state. [`Canvas`] draws with display-list commands.

use super::View;
use super::env::Env;
use super::host::{Cx, Handler, Inner, ViewError};
use super::state::{IntoLive, Live};
use choscorui_controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue, CustomControl,
    MeasureContext, PaintContext,
};
use choscorui_core::{
    DesignTokens, Dirty, DisplayList, EventReply, Measured, NodeId, Rect, SemanticNode,
    SemanticRole, SemanticState, Size,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The low-level extension point for new controls: a type that measures,
/// paints, reports semantics, and handles events through the control
/// protocol. Mount one with [`ControlView::new`].
pub trait CustomView: CustomControl + 'static {}

impl<T: CustomControl + 'static> CustomView for T {}

/// Builds a control from the environment.
type Build = Rc<dyn Fn(&Env) -> Box<dyn CustomControl>>;

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
        Self::with_source(Source::Built(Rc::new(build)))
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
    fn mount(self, cx: &mut Cx<'_>) {
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
fn mount_built(cx: &mut Cx<'_>, build: Build, current: Rc<Current>) -> NodeId {
    let env = cx.env.clone();
    let first: Rc<RefCell<Option<Box<dyn CustomControl>>>> = Rc::default();
    let mounted: Rc<Cell<Option<NodeId>>> = Rc::default();
    {
        let (first, mounted) = (first.clone(), mounted.clone());
        cx.react(move |inner| {
            let mut control = build(&env);
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

fn set_enabled(inner: &mut Inner, node: NodeId, enabled: bool) {
    if inner.controls.is_enabled(node) == enabled {
        return;
    }
    if let Err(error) = inner.controls.set_enabled(&mut inner.tree, node, enabled) {
        inner.report(ViewError::Tree(error));
    }
    inner.touch(node, Dirty::PAINT.with(Dirty::SEMANTICS));
}

/// What a [`Canvas`] draws with: the display list, its bounds, the tokens,
/// and the layout direction.
pub struct CanvasContext<'a> {
    pub list: &'a mut DisplayList,
    pub bounds: Rect,
    pub tokens: &'a DesignTokens,
    pub rtl: bool,
}

type Draw = Rc<dyn Fn(&mut CanvasContext<'_>, Size)>;

struct CanvasLeaf {
    draw: Draw,
    label: String,
}

impl CustomControl for CanvasLeaf {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max;
        let min = cx.constraints.min;
        Measured::new(Size::new(
            if max.width.is_finite() {
                max.width
            } else {
                min.width
            },
            if max.height.is_finite() {
                max.height
            } else {
                min.height
            },
        ))
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let bounds = cx.bounds;
        let mut canvas = CanvasContext {
            list: cx.list,
            bounds,
            tokens: cx.tokens,
            rtl: cx.rtl,
        };
        (self.draw)(&mut canvas, bounds.size);
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Image,
            name: self.label.clone(),
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

/// Custom drawing: `Canvas::new(|ctx, size| …)` draws with display-list
/// commands in the space the canvas gets. It fills the space it is
/// offered; give it a size with `.frame(...)`.
pub struct Canvas {
    draw: Draw,
    label: String,
}

impl Canvas {
    pub fn new(draw: impl Fn(&mut CanvasContext<'_>, Size) + 'static) -> Self {
        Self {
            draw: Rc::new(draw),
            label: String::new(),
        }
    }

    /// The name that assistive technology reads for the drawing.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

impl View for Canvas {
    fn mount(self, cx: &mut Cx<'_>) {
        let label = self.label.clone();
        let node = cx.control(Box::new(CanvasLeaf {
            draw: self.draw,
            label: self.label,
        }));
        if label.is_empty() {
            cx.inner.data(node).access.hidden = true;
        }
    }
}
