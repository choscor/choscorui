//! Static content: labels and separators.

use crate::page::{Column, Env, Page, node, sem};
use choscorui::controls::{Label, LabelStyle, Separator, SeparatorOrientation, SeparatorStyle};
use choscorui::core::{DisplayList, NodeId, Point, Rect, SemanticNode, SemanticRole, Tree};

pub struct LabelPage {
    labels: Vec<(NodeId, Label, f64, bool)>,
}

pub fn label(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows = [
        ("Heading label", 1.5, false),
        ("Body label in the theme font size", 1.0, false),
        ("Caption label, dimmed", 0.75, true),
        ("Tiếng Việt có dấu: Xin chào thế giới", 1.0, false),
        ("日本語と中文の混在テキスト", 1.0, false),
    ];
    Box::new(LabelPage {
        labels: rows
            .into_iter()
            .map(|(text, scale, dim)| {
                let id = node(tree, parent, false);
                (id, Label::new(id, text), scale, dim)
            })
            .collect(),
    })
}

impl Page for LabelPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (id, _, scale, _) in &self.labels {
            let height = (env.tokens.font_size * scale * 1.6).round();
            let _ = tree.set_bounds(*id, column.row(height));
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (id, label, scale, dim) in &self.labels {
            let Some(bounds) = tree.bounds(*id) else {
                continue;
            };
            let size = (env.tokens.font_size * scale).round();
            let width = env.measure(label.text(), size);
            let x = if env.rtl() {
                bounds.max_x() - width
            } else {
                bounds.origin.x
            };
            let style = LabelStyle {
                font_family: crate::page::FONT.into(),
                font_size: size,
                color: if *dim {
                    env.tokens.content.with_alpha(0.6)
                } else {
                    env.tokens.content
                },
            };
            let baseline = Point::new(x, bounds.origin.y + bounds.size.height / 2.0 + size * 0.35);
            let _ = style.paint(list, baseline, label);
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.labels
            .iter()
            .map(|(id, label, _, _)| (*id, label.semantics()))
            .collect()
    }
}

pub struct SeparatorPage {
    captions: Vec<NodeId>,
    horizontal: Vec<(NodeId, SeparatorStyle)>,
    vertical: Vec<(NodeId, SeparatorStyle)>,
}

pub fn separator(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let color = env.tokens.content.with_alpha(0.35);
    let styles = [1u16, 2, 4].map(|thickness| SeparatorStyle { color, thickness });
    Box::new(SeparatorPage {
        captions: (0..2).map(|_| node(tree, parent, false)).collect(),
        horizontal: styles
            .iter()
            .map(|s| (node(tree, parent, false), *s))
            .collect(),
        vertical: styles
            .iter()
            .map(|s| (node(tree, parent, false), *s))
            .collect(),
    })
}

impl Page for SeparatorPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let _ = tree.set_bounds(self.captions[0], column.row(24.0));
        for (id, _) in &self.horizontal {
            let row = column.row(16.0);
            let _ = tree.set_bounds(*id, row);
        }
        let _ = tree.set_bounds(self.captions[1], column.row(24.0));
        let cells = column.cells(120.0, &[60.0, 60.0, 60.0]);
        for ((id, _), cell) in self.vertical.iter().zip(cells) {
            let _ = tree.set_bounds(*id, cell);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let captions = [
            "Horizontal: 1, 2, and 4 points",
            "Vertical: 1, 2, and 4 points",
        ];
        for (id, text) in self.captions.iter().zip(captions) {
            env.caption(list, tree.bounds(*id).unwrap_or_default(), text);
        }
        for (list_of, orientation) in [
            (&self.horizontal, SeparatorOrientation::Horizontal),
            (&self.vertical, SeparatorOrientation::Vertical),
        ] {
            for (id, style) in list_of {
                let bounds = tree.bounds(*id).unwrap_or_default();
                style.paint(list, bounds, Separator::new(orientation));
            }
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        vec![
            (
                self.captions[0],
                sem(SemanticRole::StaticText, "Horizontal separators", None),
            ),
            (
                self.captions[1],
                sem(SemanticRole::StaticText, "Vertical separators", None),
            ),
        ]
    }
}
