//! Static content: labels and separators.

use crate::page::{Column, Env, Page, node, sem};
use choscorui::controls::{Label, Separator, SeparatorOrientation, VisualState};
use choscorui::core::{DisplayList, NodeId, Rect, SemanticNode, SemanticRole, Tree};

pub struct LabelPage {
    labels: Vec<(NodeId, Label, bool)>,
}

pub fn label(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows = [
        ("Email address", false),
        ("Disabled label", true),
        ("Tiếng Việt có dấu: Xin chào thế giới", false),
        ("日本語と中文の混在テキスト", false),
    ];
    Box::new(LabelPage {
        labels: rows
            .into_iter()
            .map(|(text, disabled)| {
                let id = node(tree, parent, false);
                (id, Label::new(id, text), disabled)
            })
            .collect(),
    })
}

impl Page for LabelPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (id, _, _) in &self.labels {
            let _ = tree.set_bounds(*id, column.row(24.0));
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (id, label, disabled) in &self.labels {
            let bounds = tree.bounds(*id).unwrap_or_default();
            let state = VisualState::NONE.set(VisualState::DISABLED, *disabled || !env.enabled);
            label.paint(&mut env.cx(list, bounds).state(state));
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.labels
            .iter()
            .map(|(id, label, _)| (*id, label.semantics()))
            .collect()
    }
}

pub struct SeparatorPage {
    captions: Vec<NodeId>,
    horizontal: Vec<NodeId>,
    vertical: Vec<NodeId>,
}

pub fn separator(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    Box::new(SeparatorPage {
        captions: (0..2).map(|_| node(tree, parent, false)).collect(),
        horizontal: (0..3).map(|_| node(tree, parent, false)).collect(),
        vertical: (0..3).map(|_| node(tree, parent, false)).collect(),
    })
}

impl Page for SeparatorPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let _ = tree.set_bounds(self.captions[0], column.row(24.0));
        for id in &self.horizontal {
            let row = column.row(16.0);
            let _ = tree.set_bounds(*id, row);
        }
        let _ = tree.set_bounds(self.captions[1], column.row(24.0));
        let cells = column.cells(120.0, &[60.0, 60.0, 60.0]);
        for (id, cell) in self.vertical.iter().zip(cells) {
            let _ = tree.set_bounds(*id, cell);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let captions = ["Horizontal", "Vertical"];
        for (id, text) in self.captions.iter().zip(captions) {
            env.caption(list, tree.bounds(*id).unwrap_or_default(), text);
        }
        for (list_of, orientation) in [
            (&self.horizontal, SeparatorOrientation::Horizontal),
            (&self.vertical, SeparatorOrientation::Vertical),
        ] {
            for id in list_of {
                let bounds = tree.bounds(*id).unwrap_or_default();
                Separator::new(orientation).paint(&mut env.cx(list, bounds));
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
