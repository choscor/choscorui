use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, Styles, TextAlign, TextMeasure, VisualState,
};
use choscorui_core::{
    DesignTokens, Dirty, NodeId, Point, Rect, SemanticNode, SemanticRole, SemanticState, Size,
    Tree, TreeError,
};

pub struct Label {
    node: NodeId,
    text: String,
}

impl Label {
    pub fn new(node: NodeId, text: impl Into<String>) -> Self {
        Self {
            node,
            text: text.into(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(
        &mut self,
        tree: &mut Tree,
        text: impl Into<String>,
    ) -> Result<bool, TreeError> {
        if !tree.contains(self.node) {
            return Err(TreeError::MissingNode);
        }
        let text = text.into();
        if self.text == text {
            return Ok(false);
        }
        self.text = text;
        tree.invalidate(self.node, Dirty::LAYOUT)?;
        tree.invalidate(self.node, Dirty::PAINT)?;
        tree.invalidate(self.node, Dirty::SEMANTICS)?;
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

    /// Paints a rule of the recipe thickness, centered across the bounds.
    pub fn paint(self, cx: &mut PaintContext<'_>) {
        let style = cx.style(StyleQuery::new(ControlKind::Separator, Slot::Root).state(cx.state));
        let bounds = cx.bounds;
        let line = match self.orientation {
            SeparatorOrientation::Horizontal => {
                let thickness = style.min_height.min(bounds.size.height);
                Rect::new(
                    Point::new(
                        bounds.origin.x,
                        bounds.origin.y + (bounds.size.height - thickness) / 2.0,
                    ),
                    Size::new(bounds.size.width, thickness),
                )
            }
            SeparatorOrientation::Vertical => {
                let thickness = style.min_width.min(bounds.size.width);
                Rect::new(
                    Point::new(
                        bounds.origin.x + (bounds.size.width - thickness) / 2.0,
                        bounds.origin.y,
                    ),
                    Size::new(thickness, bounds.size.height),
                )
            }
        };
        cx.list.fill_rect(line, style.background);
    }
}
