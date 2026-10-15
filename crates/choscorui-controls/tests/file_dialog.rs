//! The file dialog that the toolkit draws: listing, filters, hidden
//! files, keyboard navigation, the outcomes of each kind, the overwrite
//! confirmation, semantics, and the right-to-left layout.

mod hosted;

use choscorui_controls::{
    ControlMessage, EditCommand, FileDialog, FileDialogOutcome, FileDialogResult,
    ShortcutModifiers, Styles,
};
use choscorui_core::{
    DesignTokens, FileDialogKind, FileDialogRequest, Key, LayoutDirection, Localizer, Rect,
    SemanticPart, SemanticRole, ThemeResolver,
};
use hosted::Harness;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The fixture directory: `Documents/notes.txt`, `readme.txt`,
/// `report.md`, and `.hidden`.
const FILES: &str = "tests/catalog/files";

/// How long a test waits for a listing thread before it fails.
const LISTING: Duration = Duration::from_secs(10);

struct Dialog {
    h: Harness,
    result: FileDialogResult,
    /// A message for each listing that the dialog starts after its first.
    ready: Receiver<()>,
}

impl Dialog {
    fn new(dialog: FileDialog, direction: LayoutDirection) -> Self {
        let (sender, ready) = channel();
        let sender = Mutex::new(sender);
        let mut dialog = dialog.with_waker(Arc::new(move || {
            let _ = sender.lock().unwrap().send(());
        }));
        let _ = dialog.wait(LISTING);
        let result = dialog.result();
        let h = Harness::mount(
            Box::new(dialog),
            ThemeResolver::new(DesignTokens::light()),
            Styles::new(),
            direction,
        );
        Self { h, result, ready }
    }

    fn open(kind: FileDialogKind, types: &[&str]) -> Self {
        let types: Vec<String> = types.iter().map(|each| each.to_string()).collect();
        let dialog = FileDialog::new(kind, FILES, Localizer::builtin())
            .with_allowed_types(&types, Localizer::builtin());
        Self::new(dialog, LayoutDirection::Ltr)
    }

    /// Waits for the listing that the last action started, and lets the
    /// host merge it.
    fn settle(&mut self) {
        self.ready
            .recv_timeout(LISTING)
            .expect("the listing finishes");
        let time = self.h.time + 1.0;
        self.h.set_time(time);
        let _ = self.h.host.advance(&mut self.h.tree, time);
        let bounds = self.h.bounds();
        self.h.place(bounds);
    }

    fn parts(&self) -> Vec<SemanticPart> {
        self.h.parts().parts().to_vec()
    }

    /// The names of the rows in view.
    fn rows(&self) -> Vec<String> {
        self.parts()
            .into_iter()
            .filter(|part| part.node.role == SemanticRole::ListItem)
            .map(|part| part.node.name)
            .collect()
    }

    fn part(&self, role: SemanticRole, name: &str) -> SemanticPart {
        self.parts()
            .into_iter()
            .find(|part| part.node.role == role && part.node.name == name)
            .unwrap_or_else(|| panic!("no {role:?} named {name}"))
    }

    fn bounds_of(&self, role: SemanticRole, name: &str) -> Rect {
        self.part(role, name).bounds
    }

    fn click(&mut self, role: SemanticRole, name: &str) {
        let at = self.bounds_of(role, name).center();
        self.h.click(at);
    }

    /// Lays the dialog out again, as the host does before the next
    /// frame.
    fn relayout(&mut self) {
        let bounds = self.h.bounds();
        self.h.place(bounds);
    }

    /// Focuses the dialog and moves to the list, the second stop. The
    /// list has no current row yet, so the list part has the focus.
    fn focus_list(&mut self) {
        self.h.focus();
        self.h.key(Key::Tab);
        assert_eq!(self.h.focused_part(), "Files");
    }
}

fn path(name: &str) -> PathBuf {
    Path::new(FILES).join(name)
}

#[test]
fn the_list_shows_directories_first_and_the_allowed_types() {
    let d = Dialog::open(FileDialogKind::Open, &["txt", "md"]);
    assert_eq!(d.rows(), ["Documents", "readme.txt", "report.md"]);
    let filter = d.part(SemanticRole::PopUpButton, "File type");
    assert_eq!(filter.node.value.as_deref(), Some("*.txt, *.md"));
    let d = Dialog::open(FileDialogKind::Open, &["md"]);
    assert_eq!(d.rows(), ["Documents", "report.md"]);
}

#[test]
fn the_filter_cycles_through_each_type() {
    let mut d = Dialog::open(FileDialogKind::Open, &["txt", "md"]);
    d.click(SemanticRole::PopUpButton, "File type");
    assert_eq!(d.rows(), ["Documents", "readme.txt"]);
    d.click(SemanticRole::PopUpButton, "File type");
    assert_eq!(d.rows(), ["Documents", "report.md"]);
    d.click(SemanticRole::PopUpButton, "File type");
    assert_eq!(d.rows(), ["Documents", "readme.txt", "report.md"]);
}

#[test]
fn the_checkbox_shows_hidden_files() {
    let mut d = Dialog::open(FileDialogKind::Open, &[]);
    assert_eq!(d.rows(), ["Documents", "readme.txt", "report.md"]);
    let hidden = d.part(SemanticRole::Checkbox, "Show hidden files");
    assert_eq!(hidden.node.state.checked, Some(false));
    d.click(SemanticRole::Checkbox, "Show hidden files");
    d.settle();
    assert_eq!(
        d.rows(),
        ["Documents", ".hidden", "readme.txt", "report.md"]
    );
    let hidden = d.part(SemanticRole::Checkbox, "Show hidden files");
    assert_eq!(hidden.node.state.checked, Some(true));
}

#[test]
fn enter_opens_a_directory_and_backspace_goes_back() {
    let mut d = Dialog::open(FileDialogKind::Open, &[]);
    d.focus_list();
    d.h.key(Key::ArrowDown);
    assert_eq!(d.h.focused_part(), "Documents");
    d.h.key(Key::Enter);
    d.settle();
    assert_eq!(d.rows(), ["notes.txt"]);
    let location = d.part(SemanticRole::TextField, "Location");
    assert_eq!(
        location.node.value.as_deref(),
        Some(path("Documents").to_str().unwrap())
    );
    d.h.key(Key::Backspace);
    d.settle();
    assert_eq!(d.rows(), ["Documents", "readme.txt", "report.md"]);
    // Alt+Up goes to the parent from any stop.
    d.h.key(Key::ArrowDown);
    d.h.key(Key::Enter);
    d.settle();
    let shift = ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    };
    d.h.chord(shift, Key::Tab);
    assert_eq!(d.h.focused_part(), "Location");
    let option = ShortcutModifiers {
        option: true,
        ..ShortcutModifiers::default()
    };
    d.h.chord(option, Key::ArrowUp);
    d.settle();
    assert_eq!(d.rows(), ["Documents", "readme.txt", "report.md"]);
    assert!(d.result.get().is_none());
}

#[test]
fn a_typed_location_opens_its_directory() {
    let mut d = Dialog::open(FileDialogKind::Open, &[]);
    d.h.focus();
    assert_eq!(d.h.focused_part(), "Location");
    d.h.host
        .dispatch_edit(&mut d.h.tree, EditCommand::SelectAll);
    // A relative location is relative to the shown directory.
    d.h.text("Documents");
    d.h.key(Key::Enter);
    d.settle();
    assert_eq!(d.rows(), ["notes.txt"]);
}

#[test]
fn an_open_dialog_selects_one_file() {
    let mut d = Dialog::open(FileDialogKind::Open, &[]);
    d.focus_list();
    d.h.key(Key::ArrowDown);
    d.h.key(Key::ArrowDown);
    assert_eq!(d.h.focused_part(), "readme.txt");
    assert_eq!(d.h.selected_part(), "readme.txt");
    d.h.key(Key::Enter);
    assert_eq!(
        d.result.take(),
        Some(FileDialogOutcome::Selected(vec![path("readme.txt")]))
    );
    assert!(d.h.messages().contains(&ControlMessage::Activated));
}

#[test]
fn a_double_click_accepts_a_file() {
    let mut d = Dialog::open(FileDialogKind::Open, &[]);
    let at = d.bounds_of(SemanticRole::ListItem, "report.md").center();
    d.h.double_click(at);
    assert_eq!(
        d.result.take(),
        Some(FileDialogOutcome::Selected(vec![path("report.md")]))
    );
}

#[test]
fn escape_cancels() {
    let mut d = Dialog::open(FileDialogKind::Open, &[]);
    d.focus_list();
    d.h.key(Key::Escape);
    assert_eq!(d.result.take(), Some(FileDialogOutcome::Cancelled));
    assert!(d.h.messages().contains(&ControlMessage::Dismissed));
}

#[test]
fn shift_extends_the_selection_of_a_dialog_for_several_files() {
    let mut d = Dialog::open(FileDialogKind::OpenMany, &[]);
    d.focus_list();
    d.h.key(Key::ArrowDown);
    d.h.key(Key::ArrowDown);
    let shift = ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    };
    d.h.chord(shift, Key::ArrowDown);
    d.click(SemanticRole::Button, "Open");
    assert_eq!(
        d.result.take(),
        Some(FileDialogOutcome::Selected(vec![
            path("readme.txt"),
            path("report.md")
        ]))
    );
}

#[test]
fn a_directory_dialog_lists_and_returns_directories() {
    let mut d = Dialog::open(FileDialogKind::Directory, &[]);
    assert_eq!(d.rows(), ["Documents"]);
    d.click(SemanticRole::Button, "Select");
    assert_eq!(
        d.result.take(),
        Some(FileDialogOutcome::Selected(vec![PathBuf::from(FILES)]))
    );
}

#[test]
fn the_parts_follow_the_focus_order() {
    let d = Dialog::open(FileDialogKind::Open, &[]);
    let node = d.h.semantics();
    assert_eq!(
        (node.role, node.name.as_str()),
        (SemanticRole::Dialog, "Open File")
    );
    let roles: Vec<SemanticRole> = d
        .parts()
        .iter()
        .filter(|part| part.node.role != SemanticRole::ListItem)
        .map(|part| part.node.role)
        .collect();
    assert_eq!(
        roles,
        [
            SemanticRole::Button,
            SemanticRole::TextField,
            SemanticRole::List,
            SemanticRole::PopUpButton,
            SemanticRole::Checkbox,
            SemanticRole::Button,
            SemanticRole::Button,
        ]
    );
    let list = d.part(SemanticRole::List, "Files");
    assert_eq!(list.collection.map(|each| each.item_count), Some(3));
    let folder = d.part(SemanticRole::ListItem, "Documents");
    assert_eq!(folder.node.description, "Folder");
    // Nothing is selected, so Open is disabled.
    assert!(d.part(SemanticRole::Button, "Open").node.state.disabled);
}

#[test]
fn a_right_to_left_dialog_mirrors_its_layout() {
    let dialog = FileDialog::new(FileDialogKind::Open, FILES, Localizer::builtin());
    let d = Dialog::new(dialog, LayoutDirection::Rtl);
    let cancel = d.bounds_of(SemanticRole::Button, "Cancel");
    let open = d.bounds_of(SemanticRole::Button, "Open");
    assert!(open.max_x() <= cancel.min_x(), "{open:?} {cancel:?}");
    let parent = d.bounds_of(SemanticRole::Button, "Parent folder");
    let location = d.bounds_of(SemanticRole::TextField, "Location");
    assert!(
        parent.min_x() >= location.max_x(),
        "{parent:?} {location:?}"
    );
}

/// A new empty directory under the system temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "choscorui-file-dialog-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn save_dialog(dir: &Path, name: &str) -> Dialog {
    let request = FileDialogRequest {
        kind: FileDialogKind::Save,
        title: None,
        allowed_types: Vec::new(),
        suggested_name: Some(name.to_owned()),
    };
    let dialog = FileDialog::from_request(&request, dir, Localizer::builtin());
    Dialog::new(dialog, LayoutDirection::Ltr)
}

/// Moves the focus to the name field: the third stop.
fn focus_name(d: &mut Dialog) {
    d.h.focus();
    d.h.key(Key::Tab);
    d.h.key(Key::Tab);
    assert_eq!(d.h.focused_part(), "Name");
}

#[test]
fn a_save_dialog_returns_the_typed_name() {
    let dir = scratch("new");
    let mut d = save_dialog(&dir, "draft.txt");
    focus_name(&mut d);
    d.h.host
        .dispatch_edit(&mut d.h.tree, EditCommand::SelectAll);
    d.h.text("final.txt");
    d.h.key(Key::Enter);
    assert_eq!(
        d.result.take(),
        Some(FileDialogOutcome::Selected(vec![dir.join("final.txt")]))
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_save_over_a_file_asks_first() {
    let dir = scratch("replace");
    std::fs::write(dir.join("a.txt"), "old").unwrap();
    let mut d = save_dialog(&dir, "a.txt");
    focus_name(&mut d);
    d.h.key(Key::Enter);
    d.relayout();
    assert!(d.result.get().is_none());
    let alert = d.part(SemanticRole::Dialog, "“a.txt” already exists");
    assert!(alert.node.state.modal);
    assert_eq!(d.h.focused_part(), "Cancel");
    // Enter runs Cancel, never Replace.
    d.h.key(Key::Enter);
    assert!(d.result.get().is_none());
    assert!(
        d.parts()
            .iter()
            .all(|part| part.node.role != SemanticRole::Dialog)
    );
    d.h.key(Key::Enter);
    d.relayout();
    d.h.key(Key::Tab);
    assert_eq!(d.h.focused_part(), "Replace");
    d.h.key(Key::Space);
    assert_eq!(
        d.result.take(),
        Some(FileDialogOutcome::Selected(vec![dir.join("a.txt")]))
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
