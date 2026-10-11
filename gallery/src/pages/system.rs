//! Desktop integration: status items, URL handlers, and notifications,
//! offered only where the backend reports support.

use crate::page::{Align, Column, Env, Page, Push, node, sem};
use choscorui::controls::ShortcutModifiers;
use choscorui::core::{
    BackendCapabilities, ColorRole, DisplayList, KeyEvent, NodeId, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Support, Tree,
};

/// Every capability the backend reports, by display name.
pub fn capability_rows(capabilities: &BackendCapabilities) -> Vec<(&'static str, Support)> {
    vec![
        ("Multiple windows", capabilities.multiple_windows),
        ("Native menus", capabilities.native_menus),
        ("Context menus", capabilities.context_menus),
        ("Native dialogs", capabilities.native_dialogs),
        ("Clipboard", capabilities.clipboard),
        ("Drag and drop", capabilities.drag_and_drop),
        ("Input methods", capabilities.input_methods),
        ("Accessibility", capabilities.accessibility),
        ("Status items", capabilities.status_items),
        ("Notifications", capabilities.notifications),
        ("Open URLs", capabilities.open_urls),
        ("System appearance", capabilities.system_appearance),
        ("Text shaping", capabilities.text_shaping),
        ("Image codecs", capabilities.image_codecs),
        ("GPU painting", capabilities.gpu_painting),
    ]
}

fn support_text(support: Support) -> String {
    match support {
        Support::Supported => "supported".into(),
        Support::Unsupported => "not supported".into(),
        Support::Conditional(condition) => format!("needs {condition}"),
    }
}

/// The backend's capability report plus buttons for the desktop features
/// it supports; unsupported ones are disabled with the reason.
pub struct DesktopPage {
    status_item: Push,
    url: Push,
    notify: Push,
    report: NodeId,
    result: NodeId,
    last: String,
}

pub fn desktop(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let capabilities = env.desktop.capabilities();
    let make = |tree: &mut Tree, label: &str, support: Support| {
        let mut push = Push::new(tree, parent, label, env);
        if !support.is_supported() {
            let _ = push.button.set_enabled(tree, push.node, false);
        }
        push
    };
    let status_item = make(tree, "Toggle status item", capabilities.status_items);
    let url = make(tree, "Who opens https:// links?", capabilities.open_urls);
    let notify = make(tree, "Send a notification", capabilities.notifications);
    let report = node(tree, parent, false);
    let result = node(tree, parent, false);
    Box::new(DesktopPage {
        status_item,
        url,
        notify,
        report,
        result,
        last: format!("Backend: {}", capabilities.backend),
    })
}

impl DesktopPage {
    fn run(&mut self, button: usize, env: &Env<'_>) -> bool {
        self.last = match button {
            0 => match env.desktop.toggle_status_item("choscorui gallery") {
                Ok(true) => "Status item shown in the menu bar".into(),
                Ok(false) => "Status item removed".into(),
                Err(error) => format!("Status item failed: {error}"),
            },
            1 => match env.desktop.url_handler("https://example.com") {
                Ok(Some(app)) => format!("https:// links open in {app}"),
                Ok(None) => "No application handles https:// links".into(),
                Err(error) => format!("URL handler lookup failed: {error}"),
            },
            _ => match env
                .desktop
                .notify("choscorui gallery", "Hello from the Desktop page")
            {
                Ok(()) => "Notification sent".into(),
                Err(error) => format!("Notification failed: {error}"),
            },
        };
        true
    }

    fn buttons(&mut self) -> [&mut Push; 3] {
        [&mut self.status_item, &mut self.url, &mut self.notify]
    }
}

impl Page for DesktopPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let cells = column.cells(30.0, &[170.0, 210.0, 180.0]);
        for (push, cell) in self.buttons().into_iter().zip(cells) {
            let _ = tree.set_bounds(push.node, cell);
        }
        let _ = tree.set_bounds(self.result, column.row(22.0));
        let _ = tree.set_bounds(self.report, column.rest());
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for push in [&self.status_item, &self.url, &self.notify] {
            push.paint(tree, list, env);
        }
        env.caption(
            list,
            tree.bounds(self.result).unwrap_or_default(),
            &self.last,
        );
        let report = tree.bounds(self.report).unwrap_or_default();
        let mut column = Column::new(report, env);
        column.gap = 2.0;
        for (name, support) in capability_rows(&env.desktop.capabilities()) {
            let cells = column.cells(20.0, &[180.0, 220.0]);
            env.text(
                list,
                cells[0],
                name,
                env.small(),
                env.tokens.color(ColorRole::Foreground),
                Align::Leading,
            );
            let color = if support.is_supported() {
                env.tokens.color(ColorRole::Foreground)
            } else {
                env.tokens.color(ColorRole::Foreground).with_alpha(0.5)
            };
            env.text(
                list,
                cells[1],
                &support_text(support),
                env.small(),
                color,
                Align::Leading,
            );
        }
    }

    fn semantics(&self, tree: &Tree, env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let capabilities = env.desktop.capabilities();
        let report = capability_rows(&capabilities)
            .into_iter()
            .map(|(name, support)| format!("{name}: {}", support_text(support)))
            .collect::<Vec<_>>()
            .join("; ");
        let mut nodes = Vec::new();
        for (push, support) in [
            (&self.status_item, capabilities.status_items),
            (&self.url, capabilities.open_urls),
            (&self.notify, capabilities.notifications),
        ] {
            let (id, mut node) = push.semantics(tree);
            if !support.is_supported() {
                node.description = support_text(support);
            }
            nodes.push((id, node));
        }
        nodes.push((
            self.result,
            sem(SemanticRole::StaticText, self.last.clone(), None),
        ));
        nodes.push((
            self.report,
            sem(
                SemanticRole::StaticText,
                format!("Capabilities of the {} backend", capabilities.backend),
                Some(report),
            ),
        ));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let hit = self
            .buttons()
            .into_iter()
            .position(|push| push.pointer(tree, event));
        hit.is_some_and(|index| self.run(index, env))
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let hit = self
            .buttons()
            .into_iter()
            .position(|push| push.key(tree, event));
        hit.is_some_and(|index| self.run(index, env))
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        env: &Env<'_>,
    ) -> bool {
        let hit = self
            .buttons()
            .into_iter()
            .position(|push| push.action(tree, node, action));
        hit.is_some_and(|index| self.run(index, env))
    }

    fn native_disabled(&self) -> bool {
        true
    }
}
