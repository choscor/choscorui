//! A page of captioned rows of controls mounted through `ControlHost`.
//! Pages for the hosted components describe their rows; this page lays
//! them out, paints them, and routes input to the host.

use crate::page::{Column, Env, Page, node};
use choscorui::controls::{ControlHost, CustomControl, ShortcutModifiers};
use choscorui::core::{
    Constraints, DisplayList, Key, KeyEvent, NodeId, Point, PointerEvent, Rect, SemanticAction,
    SemanticNode, Size, ThemeResolver, Tree,
};

/// A caption and the controls shown under it.
pub type Row = (&'static str, Vec<Box<dyn CustomControl>>);

pub struct HostedPage {
    pub host: ControlHost,
    /// Each row's caption node, caption, and control nodes.
    rows: Vec<(NodeId, &'static str, Vec<NodeId>)>,
    enabled: bool,
}

impl HostedPage {
    pub fn build(tree: &mut Tree, parent: NodeId, rows: Vec<Row>) -> Box<dyn Page> {
        let mut host = ControlHost::new();
        let rows = rows
            .into_iter()
            .map(|(caption, controls)| {
                let label = node(tree, parent, false);
                let ids = controls
                    .into_iter()
                    .map(|control| {
                        host.mount(tree, parent, control)
                            .expect("parent is attached")
                    })
                    .collect();
                (label, caption, ids)
            })
            .collect();
        Box::new(Self {
            host,
            rows,
            enabled: true,
        })
    }

    /// The control nodes, in row order.
    pub fn controls(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.rows.iter().flat_map(|(_, _, ids)| ids.iter().copied())
    }
}

impl Page for HostedPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        if self.enabled != env.enabled {
            self.enabled = env.enabled;
            let ids: Vec<_> = self.controls().collect();
            for id in ids {
                let _ = self.host.set_enabled(tree, id, env.enabled);
            }
        }
        let themes = ThemeResolver::new(*env.tokens);
        let host_env = env.host(&themes);
        let mut column = Column::new(bounds, env);
        let gap = env.tokens.spacing * 2.0;
        for (caption, _, ids) in &self.rows {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let sizes: Vec<Size> = ids
                .iter()
                .map(|id| {
                    self.host
                        .measure(tree, *id, Constraints::UNBOUNDED, &host_env)
                        .map_or(Size::new(160.0, 32.0), |measured| measured.size)
                })
                .collect();
            // Wrap controls into lines that fit the page width.
            let mut start = 0;
            while start < ids.len() {
                let mut end = start;
                let mut width = 0.0;
                while end < ids.len() {
                    let next = width + sizes[end].width + if end > start { gap } else { 0.0 };
                    if end > start && next > bounds.size.width {
                        break;
                    }
                    width = next;
                    end += 1;
                }
                let height = sizes[start..end]
                    .iter()
                    .fold(0.0_f64, |tallest, size| tallest.max(size.height));
                let line = column.row(height);
                let mut x = 0.0;
                for (id, size) in ids[start..end].iter().zip(&sizes[start..end]) {
                    let left = if env.rtl() {
                        line.max_x() - x - size.width
                    } else {
                        line.origin.x + x
                    };
                    let top = line.origin.y + (height - size.height) / 2.0;
                    let _ = tree.set_bounds(*id, Rect::new(Point::new(left, top), *size));
                    x += size.width + gap;
                }
                start = end;
            }
        }
        self.host.arrange(tree, &host_env);
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, text, _) in &self.rows {
            env.caption(list, tree.bounds(*caption).unwrap_or_default(), text);
        }
        let themes = ThemeResolver::new(*env.tokens);
        self.host.paint(tree, list, &env.host(&themes));
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.controls()
            .filter_map(|id| Some((id, self.host.get(id)?.semantics())))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        self.host.dispatch_pointer(tree, event);
        // The page reads values through semantics; drop the change reports.
        self.host.take_messages();
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let focus = tree.focus();
        if !focus.is_some_and(|id| self.host.contains(id))
            || matches!(event, KeyEvent::Down { key: Key::Tab, .. })
        {
            return false;
        }
        self.host.dispatch_key(tree, event);
        self.host.take_messages();
        true
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        let handled = self.host.dispatch_text(tree, text);
        self.host.take_messages();
        handled
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let performed = self.host.perform(tree, node, action, value);
        self.host.take_messages();
        performed
    }

    fn native_disabled(&self) -> bool {
        true
    }
}
