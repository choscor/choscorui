use choscorui_core::{
    Color, Dirty, DisplayList, DisplayListError, NodeId, Point, Rect, SemanticNode, SemanticRole,
    SemanticState, Tree, TreeError,
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

#[derive(Clone, Debug, PartialEq)]
pub struct LabelStyle {
    pub font_family: String,
    pub font_size: f64,
    pub color: Color,
}

impl LabelStyle {
    pub fn paint(
        &self,
        list: &mut DisplayList,
        baseline: Point,
        label: &Label,
    ) -> Result<(), DisplayListError> {
        list.draw_text(
            baseline,
            label.text(),
            &self.font_family,
            self.font_size,
            self.color,
        )
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SeparatorStyle {
    pub color: Color,
    pub thickness: u16,
}

impl SeparatorStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, separator: Separator) {
        let thickness = f64::from(self.thickness).min(match separator.orientation() {
            SeparatorOrientation::Horizontal => bounds.size.height,
            SeparatorOrientation::Vertical => bounds.size.width,
        });
        let line = match separator.orientation() {
            SeparatorOrientation::Horizontal => Rect::new(
                Point::new(
                    bounds.origin.x,
                    bounds.origin.y + (bounds.size.height - thickness) / 2.0,
                ),
                choscorui_core::Size::new(bounds.size.width, thickness),
            ),
            SeparatorOrientation::Vertical => Rect::new(
                Point::new(
                    bounds.origin.x + (bounds.size.width - thickness) / 2.0,
                    bounds.origin.y,
                ),
                choscorui_core::Size::new(thickness, bounds.size.height),
            ),
        };
        list.fill_rect(line, self.color);
    }
}
