//! The tool box page: an accordion of tool pages, one open at a time.

use crate::ui::page::{PageCx, column, section, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{AnyView, Binding, Env, Frame, ToolBox, VStack, ViewExt};

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

/// The tool box with the tool pages in `env`, and `open` as the open
/// page. The tool box paints the text of the open page.
fn tool_box_pages(open: Binding<usize>) -> ToolBox {
    let titles = |env: &Env| {
        tool_pages(env)
            .into_iter()
            .map(|(title, _)| title)
            .collect()
    };
    ToolBox::new(titles, open).with_env(|toolbox, env| {
        let texts = tool_pages(env).into_iter().map(|(_, text)| text).collect();
        toolbox.with_page_texts(texts)
    })
}

/// An accordion: Up and Down move between headers, Space opens one. The
/// group reads the open page's title and text.
pub fn tool_box(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (open, disabled) = (cx.state(0usize), cx.state(0usize));
        let toolbox = tool_box_pages(open.binding());
        let pages = tool_pages(cx.env());
        let hints = pages.clone();
        column((
            VStack::new(toolbox.frame(Frame::size(320.0, 260.0)))
                .accessibility_label(cx.env().text("tool-box.name", &[]))
                .accessibility_value(move || pages[open.get()].0.clone())
                .accessibility_hint(move || hints[open.get()].1.clone()),
            section(
                t!("tool-box.disabled"),
                tool_box_pages(disabled.binding())
                    .frame(Frame::size(320.0, 200.0))
                    .disabled(true),
            ),
        ))
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
