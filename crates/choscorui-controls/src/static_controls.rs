use crate::style::fill_area;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, Styles, TextAlign, TextMeasure, VisualState,
};
use choscorui_core::{
    DesignTokens, Dirty, NodeId, Point, Rect, SemanticNode, SemanticRole, SemanticState, Size,
    Tree, TreeError,
};

pub struct Label {
    /// The node that `set_text` invalidates. A hosted label has none.
    node: Option<NodeId>,
    text: String,
    /// Whether the label names a control, so that a click on it acts.
    target: bool,
    /// Whether a primary press on the label is down.
    armed: bool,
}

impl Label {
    pub fn new(node: NodeId, text: impl Into<String>) -> Self {
        Self {
            node: Some(node),
            text: text.into(),
            target: false,
            armed: false,
        }
    }

    /// A label for a control host, which owns the node. The host changes
    /// the text by mounting a new label, so `set_text` fails on this one.
    pub fn hosted(text: impl Into<String>) -> Self {
        Self {
            node: None,
            text: text.into(),
            target: false,
            armed: false,
        }
    }

    /// The label names a control: in a control host, a primary click on
    /// it emits `Activated`, and the view layer focuses or toggles the
    /// control.
    #[must_use]
    pub fn with_target(mut self) -> Self {
        self.target = true;
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(
        &mut self,
        tree: &mut Tree,
        text: impl Into<String>,
    ) -> Result<bool, TreeError> {
        let Some(node) = self.node.filter(|node| tree.contains(*node)) else {
            return Err(TreeError::MissingNode);
        };
        let text = text.into();
        if self.text == text {
            return Ok(false);
        }
        self.text = text;
        tree.invalidate(node, Dirty::LAYOUT)?;
        tree.invalidate(node, Dirty::PAINT)?;
        tree.invalidate(node, Dirty::SEMANTICS)?;
        Ok(true)
    }

    fn query(state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Label, Slot::Root).state(state)
    }

    /// The natural size of the text in the label recipe.
    pub fn measure(&self, styles: &Styles, tokens: &DesignTokens, text: &dyn TextMeasure) -> Size {
        let style = styles.resolve(tokens, Self::query(VisualState::NONE));
        Size::new(text.width(&self.text, &style.text), style.text.line_height)
    }

    /// Paints the text at the start of the bounds, vertically centered.
    /// A `DISABLED` state in the context dims it.
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(Self::query(cx.state));
        let bounds = cx.bounds;
        cx.slot(bounds, &style, |cx, rect| {
            cx.label(rect, &self.text, &style, TextAlign::Start);
        });
    }

    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::StaticText,
            name: self.text.clone(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeparatorOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Separator {
    orientation: SeparatorOrientation,
}

impl Separator {
    pub const fn new(orientation: SeparatorOrientation) -> Self {
        Self { orientation }
    }

    pub const fn orientation(self) -> SeparatorOrientation {
        self.orientation
    }

    /// Paints a rule of the recipe thickness, centered across the bounds
    /// and snapped to whole points, so that it stays sharp at 1x and 2x.
    pub fn paint(self, cx: &mut PaintContext<'_>) {
        let style = cx.style(StyleQuery::new(ControlKind::Separator, Slot::Root).state(cx.state));
        let bounds = cx.bounds;
        let line = match self.orientation {
            SeparatorOrientation::Horizontal => {
                let thickness = style.min_height.min(bounds.size.height);
                Rect::new(
                    Point::new(
                        bounds.origin.x,
                        (bounds.origin.y + (bounds.size.height - thickness) / 2.0).round(),
                    ),
                    Size::new(bounds.size.width, thickness),
                )
            }
            SeparatorOrientation::Vertical => {
                let thickness = style.min_width.min(bounds.size.width);
                Rect::new(
                    Point::new(
                        (bounds.origin.x + (bounds.size.width - thickness) / 2.0).round(),
                        bounds.origin.y,
                    ),
                    Size::new(thickness, bounds.size.height),
                )
            }
        };
        fill_area(cx.list, line, cx.rtl, &style.background);
    }
}

impl crate::CustomControl for Label {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let size = Label::measure(self, cx.styles, cx.tokens, cx.text);
        choscorui_core::Measured::new(cx.constraints.constrain(size))
    }

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        use crate::{ControlEvent, ControlMessage};
        use choscorui_core::{EventPhase, EventReply, PointerButton, PointerEvent};
        if !self.target || cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                self.armed = true;
                cx.capture_pointer(true);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up {
                position,
                button: PointerButton::Primary,
            }) if self.armed => {
                self.armed = false;
                cx.capture_pointer(false);
                if cx.bounds().contains(*position) {
                    cx.emit(ControlMessage::Activated);
                }
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.armed = false;
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        Label::paint(self, cx);
    }

    fn semantics(&self) -> SemanticNode {
        Label::semantics(self)
    }

    fn focusable(&self) -> bool {
        false
    }
}

impl crate::CustomControl for Separator {
    /// The recipe thickness across the rule, and the width or height it
    /// is given along it.
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Separator, Slot::Root));
        let along = |max: f64| if max.is_finite() { max } else { 0.0 };
        let max = cx.constraints.max;
        let size = match self.orientation {
            SeparatorOrientation::Horizontal => Size::new(along(max.width), style.min_height),
            SeparatorOrientation::Vertical => Size::new(style.min_width, along(max.height)),
        };
        choscorui_core::Measured::new(cx.constraints.constrain(size))
    }

    fn event(
        &mut self,
        _cx: &mut crate::ControlContext<'_>,
        _event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        choscorui_core::EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        Separator::paint(*self, cx);
    }

    /// A hidden group: the rule is decorative.
    fn semantics(&self) -> SemanticNode {
        let mut node = crate::hosted::node(SemanticRole::Group, "", Vec::new());
        node.state.hidden = true;
        node
    }

    fn focusable(&self) -> bool {
        false
    }
}
