//! The tool box page: an accordion of tool pages, one open at a time.

use crate::ui::page::{PageCx, column, stateful};
use choscorui::compose::view::{AnyView, Configured, Env, Frame, VStack, ViewExt};
use choscorui::controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue, CustomControl,
    MeasureContext, PaintContext, Slot, StyleQuery, TextAlign, ToolBox, paint_wrapped,
};
use choscorui::core::{EventReply, Measured, Rect, SemanticAction, SemanticNode};

/// The title and text keys of each tool page.
const TOOL_PAGES: [(&str, &str); 3] = [
    ("tool-box.brushes", "tool-box.brushes_text"),
    ("tool-box.layers", "tool-box.layers_text"),
    ("tool-box.history", "tool-box.history_text"),
];

/// The title and text of each tool page in `env`.
fn tool_pages(env: &Env) -> Vec<(String, String)> {
    TOOL_PAGES
        .iter()
        .map(|(title, text)| (env.text(title, &[]), env.text(text, &[])))
        .collect()
}

/// The tool box with the open page's text, which the host paints.
struct ToolBoxPages {
    toolbox: ToolBox,
    /// Each page's text.
    texts: Vec<String>,
    /// The header height from the last arrange.
    header: f64,
}

impl CustomControl for ToolBoxPages {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.toolbox.measure(cx)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.header = self.toolbox.header_height(cx.styles, cx.tokens);
        CustomControl::arrange(&mut self.toolbox, cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.toolbox.event(cx, event)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        CustomControl::paint(&self.toolbox, cx);
        let (_, page) = self.toolbox.layout(cx.bounds, self.header);
        let style = cx.style(StyleQuery::new(ControlKind::Accordion, Slot::Content));
        let text = self
            .texts
            .get(self.toolbox.current())
            .map_or("", String::as_str);
        // The text wraps in the page, and lines past its end are cut.
        cx.list.push_clip(page);
        paint_wrapped(cx, page, text, &style, TextAlign::Start);
        let _ = cx.list.pop_clip();
    }

    /// The focused header, which assistive technology can also focus.
    fn semantics(&self) -> SemanticNode {
        let mut node = CustomControl::semantics(&self.toolbox);
        node.actions.push(SemanticAction::Focus);
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        if action == SemanticAction::Focus {
            cx.request_focus();
            return true;
        }
        self.toolbox.perform(cx, action, value)
    }

    fn focusable(&self) -> bool {
        CustomControl::focusable(&self.toolbox)
    }

    fn set_enabled(&mut self, enabled: bool) {
        CustomControl::set_enabled(&mut self.toolbox, enabled);
    }
}

/// An accordion: Up and Down move between headers, Space opens one. The
/// group reads the open page's title and text.
pub fn tool_box(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let open = cx.state(0usize);
        let toolbox = Configured::custom(ControlKind::Accordion, |env| {
            let (titles, texts) = tool_pages(env).into_iter().unzip();
            ToolBoxPages {
                toolbox: ToolBox::new(titles),
                texts,
                header: 0.0,
            }
        })
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Index(Some(page))) = message {
                open.set(*page);
            }
        });
        let pages = tool_pages(cx.env());
        let hints = pages.clone();
        column(
            VStack::new(toolbox.frame(Frame::size(320.0, 260.0)))
                .accessibility_label(cx.env().text("tool-box.name", &[]))
                .accessibility_value(move || pages[open.get()].0.clone())
                .accessibility_hint(move || hints[open.get()].1.clone()),
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction};

    #[test]
    fn arrow_keys_move_between_headers_and_space_opens_one() {
        let mut page = Harness::new("tool-box");
        assert_eq!(page.value("Tool box").as_deref(), Some("Brushes"));
        assert!(page.act("Brushes", SemanticAction::Focus));
        for key in [Key::ArrowDown, Key::Space] {
            let event = KeyEvent::Down { key, repeat: false };
            assert!(page.host.key(event, ShortcutModifiers::default()));
        }
        assert_eq!(page.value("Tool box").as_deref(), Some("Layers"));
        let layers = page.node("Layers");
        let node = page.host.semantic(layers).unwrap();
        assert_eq!(node.state.expanded, Some(true));
    }
}
