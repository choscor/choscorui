//! Window chrome that the toolkit draws when the desktop does not: the
//! title bar of a window with client-side decorations.

use crate::ui::page::{Build, PageCx, column, row, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Cx, Env, Frame, State, Text, TitleBar, Toggle, View, ViewExt,
};
use choscorui::controls::{TitleBarButton, TitleBarHandle, TitleBarRequest};
use choscorui::core::{ColorRole, TextPreset};

/// The catalog key that names `request`.
fn request_key(request: TitleBarRequest) -> &'static str {
    match request {
        TitleBarRequest::Move => "title-bar.move",
        TitleBarRequest::Resize(_) => "title-bar.resize",
        TitleBarRequest::Minimize => "title-bar.minimize",
        TitleBarRequest::ToggleMaximize => "title-bar.toggle_maximize",
        TitleBarRequest::Close => "title-bar.close",
        TitleBarRequest::ShowWindowMenu(_) => "title-bar.menu",
    }
}

/// A small muted line.
fn readout(text: State<String>) -> impl View {
    Text::new(move || text.get())
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

/// A title bar that reports each request. Maximize toggles the maximized
/// state, as a window manager would.
fn window_bar(
    env: &Env,
    maximized: State<bool>,
    active: State<bool>,
    last: State<String>,
) -> impl View {
    let handle = TitleBarHandle::new();
    let requests = handle.clone();
    let env = env.clone();
    TitleBar::new(t!("title-bar.title"), handle)
        .with(move |bar| {
            bar.handle().set_maximized(maximized.get());
            bar.handle().set_active(active.get());
            bar
        })
        .on_activate(move || {
            for request in requests.take_requests() {
                if request == TitleBarRequest::ToggleMaximize {
                    maximized.update(|maximized| *maximized = !*maximized);
                }
                let name = env.text(request_key(request), &[]);
                last.set(env.text("title-bar.request", &[("request", &name)]));
            }
        })
        .frame(Frame::new().width(520.0))
}

/// The title bar of a window, and the bar of a dialog window with only a
/// close button.
struct TitleBarPage;

impl View for TitleBarPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let maximized = cx.state(false);
        let active = cx.state(true);
        let last = cx.state(cx.env().text("title-bar.none", &[]));
        let env = cx.env().clone();
        let dialog = TitleBar::new(t!("title-bar.dialog"), TitleBarHandle::new())
            .with(|bar| {
                bar.handle().set_active(false);
                bar.handle().set_maximized(true);
                bar.with_buttons(&[TitleBarButton::Close])
            })
            .frame(Frame::new().width(520.0));
        column((
            row((
                Toggle::new(t!("title-bar.maximized"), maximized.binding()),
                Toggle::new(t!("title-bar.active"), active.binding()),
            )),
            section(
                t!("title-bar.window"),
                window_bar(&env, maximized, active, last),
            ),
            readout(last),
            section(t!("title-bar.dialog_window"), dialog),
        ))
    }
}

pub fn title_bar(_: &PageCx) -> AnyView {
    TitleBarPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "title-bar" => title_bar,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::core::SemanticAction;

    /// Presses the part named `name`, as an assistive tool does.
    fn press(page: &mut Harness, name: &str) {
        let (snapshot, _) = page.host.semantics();
        let entry = snapshot
            .into_iter()
            .find(|entry| entry.node.name == name && entry.part.is_some())
            .unwrap_or_else(|| panic!("no part named {name:?}"));
        let part = entry.part.unwrap_or_default();
        assert!(
            page.host
                .perform_part(entry.id, part, SemanticAction::Activate)
        );
        page.host.layout();
    }

    #[test]
    fn the_title_bar_reports_its_requests() {
        let mut page = Harness::new("title-bar");
        assert!(page.host.find("No request yet").is_some());
        press(&mut page, "Maximize");
        assert!(page.host.find("Request: Maximize or restore").is_some());
        let maximized = page.node("Maximized");
        assert_eq!(
            page.host.semantic(maximized).unwrap().state.checked,
            Some(true)
        );
        // The maximized window shows Restore in place of Maximize.
        press(&mut page, "Restore");
        press(&mut page, "Minimize");
        assert!(page.host.find("Request: Minimize").is_some());
    }
}
