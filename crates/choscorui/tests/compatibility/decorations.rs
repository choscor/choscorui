//! The client-side decorations: the display list of the title bar of a
//! window and of a dialog, active, inactive, and maximized, in both
//! directions. A right-to-left bar mirrors the order of the buttons. The
//! frame below the content pins its margin, input area, and paint.

use super::{golden, rect};
use choscorui::controls::{
    ControlHost, EstimatedText, HostEnv, PaintContext, Styles, TitleBar, TitleBarButton,
    TitleBarHandle, WindowFrame,
};
use choscorui::core::{DesignTokens, DisplayList, LayoutDirection, ThemeResolver, Tree};
use std::fmt::Write;

/// A case: its name, the buttons of the bar, and the active and
/// maximized state of the window.
type Case = (&'static str, &'static [TitleBarButton], bool, bool);

const CASES: [Case; 4] = [
    ("window", &TitleBarButton::ALL, true, false),
    ("inactive", &TitleBarButton::ALL, false, false),
    ("maximized", &TitleBarButton::ALL, true, true),
    ("dialog", &[TitleBarButton::Close], true, false),
];

#[test]
fn decorations() {
    let mut out = String::new();
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let env = HostEnv::new(&themes, &styles, &EstimatedText).direction(direction);
        let height = TitleBar::height(&styles, &DesignTokens::light());
        for (name, buttons, active, maximized) in CASES {
            let handle = TitleBarHandle::new();
            handle.set_active(active);
            handle.set_maximized(maximized);
            let bar = TitleBar::new("Notes", handle).with_buttons(buttons);
            let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
            let mut host = ControlHost::new();
            let root = tree.root();
            let node = host.mount(&mut tree, root, Box::new(bar)).unwrap();
            tree.set_bounds(node, rect(0.0, 0.0, 400.0, height))
                .unwrap();
            host.arrange(&tree, &env);
            writeln!(out, "# {name} {direction:?}").unwrap();
            let mut list = DisplayList::new();
            host.paint(&tree, &mut list, &env);
            for command in list.commands() {
                writeln!(out, "{command:?}").unwrap();
            }
        }
    }
    frames(&mut out);
    golden("decorations", &out);
}

/// The frame below the content of a window: its margin, and its shadow,
/// background, and border when active, inactive, and maximized. A frame
/// has no direction.
fn frames(out: &mut String) {
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let window = rect(0.0, 0.0, 400.0, 300.0);
    for (name, _, active, maximized) in &CASES[..3] {
        let frame = WindowFrame::new(window, rect(0.0, 0.0, 400.0, 40.0), Vec::new())
            .with_theme_edges(&tokens)
            .with_maximized(*maximized);
        writeln!(out, "# frame {name}").unwrap();
        writeln!(out, "margin {:?}", frame.margin(&styles, &tokens)).unwrap();
        writeln!(out, "input {:?}", frame.input_area()).unwrap();
        let mut list = DisplayList::new();
        let mut cx = PaintContext::new(&mut list, window, &tokens, &styles, &EstimatedText);
        frame.paint(&mut cx, *active);
        for command in list.commands() {
            writeln!(out, "{command:?}").unwrap();
        }
    }
}
