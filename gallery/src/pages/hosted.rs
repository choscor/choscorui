//! A page of captioned rows of controls mounted through `ControlHost`.
//! Pages for the hosted components describe their rows; this page lays
//! them out, paints them, and routes input to the host.

use crate::page::{Column, Env, Page, node};
use choscorui::controls::{ControlHost, CustomControl, Field, FieldOrientation, ShortcutModifiers};
use std::cell::Cell;

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
    /// A field orientation per row lays its controls out as one form
    /// field; `None` wraps them in lines.
    fields: Vec<Option<FieldOrientation>>,
    enabled: bool,
    /// Whether a control still animated at the last paint.
    busy: Cell<bool>,
}

impl HostedPage {
    pub fn build(tree: &mut Tree, parent: NodeId, rows: Vec<Row>) -> Box<dyn Page> {
        let rows = rows.into_iter().map(|row| (row, None)).collect();
        Self::build_fields(tree, parent, rows)
    }

    /// Rows whose controls form one field each, in the given orientation.
    pub fn build_fields(
        tree: &mut Tree,
        parent: NodeId,
        rows: Vec<(Row, Option<FieldOrientation>)>,
    ) -> Box<dyn Page> {
        let fields = rows.iter().map(|(_, field)| *field).collect();
        let rows = rows.into_iter().map(|(row, _)| row);
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
            fields,
            enabled: true,
            busy: Cell::new(false),
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
        for ((caption, _, ids), field) in self.rows.iter().zip(&self.fields) {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            if let Some(orientation) = field {
                // A field is at most 480 pt wide, and its parts wrap to it.
                let width = bounds.size.width.min(480.0);
                let constraints = Constraints::loose(Size::new(width, f64::INFINITY));
                let sizes: Vec<Size> = ids
                    .iter()
                    .map(|id| {
                        self.host
                            .measure(tree, *id, constraints, &host_env)
                            .map_or(Size::new(width, 20.0), |measured| measured.size)
                    })
                    .collect();
                let field = Field::new(*orientation);
                let height = field.height(env.styles(), env.tokens, width, &sizes);
                let row = column.row(height);
                let area = Rect::new(
                    Point::new(
                        if env.rtl() {
                            row.max_x() - width
                        } else {
                            row.origin.x
                        },
                        row.origin.y,
                    ),
                    Size::new(width, height),
                );
                let rects = field.layout(env.styles(), env.tokens, area, env.rtl(), &sizes);
                for (id, rect) in ids.iter().zip(rects) {
                    let _ = tree.set_bounds(*id, rect);
                }
                continue;
            }
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
        self.busy
            .set(self.host.animating(tree, env.now.as_secs_f64()));
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
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        self.host.set_time(env.now.as_secs_f64());
        self.host.set_modifiers(modifiers);
        self.host.dispatch_pointer(tree, event);
        // The page reads values through semantics; drop the change reports.
        self.host.take_messages();
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        self.host.set_time(env.now.as_secs_f64());
        self.host.set_modifiers(modifiers);
        let focus = tree.focus();
        if !focus.is_some_and(|id| self.host.contains(id)) {
            return false;
        }
        // Tab moves focus between pages' nodes unless a control keeps it,
        // as a range slider does between its thumbs.
        let tab = matches!(event, KeyEvent::Down { key: Key::Tab, .. });
        let stopped = self.host.dispatch_key(tree, event);
        self.host.take_messages();
        stopped || !tab
    }

    fn text(&mut self, tree: &mut Tree, text: &str, env: &Env<'_>) -> bool {
        self.host.set_time(env.now.as_secs_f64());
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
        env: &Env<'_>,
    ) -> bool {
        self.host.set_time(env.now.as_secs_f64());
        let performed = self.host.perform(tree, node, action, value);
        self.host.take_messages();
        performed
    }

    #[cfg(target_os = "macos")]
    fn tick(&mut self, _env: &Env<'_>) -> bool {
        self.busy.get()
    }

    fn native_disabled(&self) -> bool {
        true
    }
}
