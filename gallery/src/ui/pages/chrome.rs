//! Window chrome that the toolkit draws when the desktop does not: the
//! title bar of a window with client-side decorations, and the file
//! dialog for a desktop without a file chooser service.

use crate::ui::page::{Build, PageCx, column, row, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Button, Cx, Dynamic, Env, FileDialog, Frame, State, Text, TitleBar, Toggle, View,
    ViewExt,
};
use choscorui::controls::{
    ButtonVariant, FileDialogOutcome, TitleBarButton, TitleBarHandle, TitleBarRequest,
};
use choscorui::core::{ColorRole, FileDialogKind, FileDialogRequest, TextPreset};
use std::path::PathBuf;

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

/// The kinds of the file dialog, with the catalog keys of their buttons.
const KINDS: [(&str, FileDialogKind); 4] = [
    ("file-dialog.open", FileDialogKind::Open),
    ("file-dialog.open_many", FileDialogKind::OpenMany),
    ("file-dialog.directory", FileDialogKind::Directory),
    ("file-dialog.save", FileDialogKind::Save),
];

/// The readout of `outcome`.
fn outcome_text(env: &Env, outcome: Option<FileDialogOutcome>) -> String {
    match outcome {
        None => env.text("file-dialog.none", &[]),
        Some(FileDialogOutcome::Cancelled) => env.text("file-dialog.cancelled", &[]),
        Some(FileDialogOutcome::Selected(paths)) => {
            let paths: Vec<String> = paths
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            env.text("file-dialog.chose", &[("paths", &paths.join(", "))])
        }
    }
}

/// The file dialog of each kind over the working directory, with the
/// outcome of the last accept or cancel.
struct FileDialogPage;

impl View for FileDialogPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let kind = cx.state(0usize);
        let outcome = cx.state(None::<FileDialogOutcome>);
        let env = cx.env().clone();
        let directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let buttons = KINDS.map(|(key, _)| {
            let index = KINDS.iter().position(|(each, _)| *each == key).unwrap_or(0);
            Button::new(t!(key), move || {
                kind.set(index);
                outcome.set(None);
            })
            .variant(ButtonVariant::Outline)
        });
        let [open, many, folder, save] = buttons;
        let suggested = env.text("file-dialog.suggested", &[]);
        let dialog = Dynamic::new(move || {
            let (_, kind) = KINDS[kind.get()];
            let request = FileDialogRequest {
                kind,
                title: None,
                allowed_types: Vec::new(),
                suggested_name: (kind == FileDialogKind::Save).then(|| suggested.clone()),
            };
            FileDialog::new(request, directory.clone(), outcome.binding())
                .frame(Frame::size(560.0, 420.0))
                .background(ColorRole::Card)
                .border(ColorRole::Border, 1.0)
        });
        column((
            row((open, many, folder, save)),
            dialog,
            Text::new(move || outcome_text(&env, outcome.get()))
                .text_style(TextPreset::Small)
                .foreground(ColorRole::MutedForeground),
        ))
    }
}

pub fn file_dialog(_: &PageCx) -> AnyView {
    FileDialogPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "title-bar" => title_bar,
        "file-dialog" => file_dialog,
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

    #[test]
    fn the_file_dialog_reports_a_cancel() {
        let mut page = Harness::new("file-dialog");
        assert!(page.host.find("No choice yet").is_some());
        press(&mut page, "Cancel");
        assert!(page.host.find("Cancelled").is_some());
        page.act("Save file", SemanticAction::Activate);
        assert!(page.host.find("Save File").is_some());
    }
}
